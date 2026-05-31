use anyhow::Result;
use std::collections::HashMap;
use std::fmt;
use tokio::process::Command;

use super::connection::SshSession;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPort {
    pub port: u16,
    pub bind_address: String,
    pub process_name: Option<String>,
    pub pid: Option<u32>,
}

impl fmt::Display for DiscoveredPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} ({})",
            self.bind_address,
            self.port,
            self.process_name.as_deref().unwrap_or("unknown")
        )
    }
}

/// Parse `ss -tlnp` output and return ports bound to 0.0.0.0 or [::].
/// Filters out localhost-only listeners (127.0.0.1, ::1).
pub fn parse_ss_output(output: &str) -> Vec<DiscoveredPort> {
    let mut ports = Vec::new();

    for line in output.lines().skip(1) {
        let line = line.trim();
        if !line.starts_with("LISTEN") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let local_addr = parts[3];
        let (bind_address, port) = parse_address_port(local_addr);

        let port = match port {
            Some(p) => p,
            None => continue,
        };

        let (process_name, pid) = if parts.len() >= 6 {
            parse_ss_process_info(parts[5])
        } else {
            (None, None)
        };

        ports.push(DiscoveredPort {
            port,
            bind_address: bind_address.to_string(),
            process_name,
            pid,
        });
    }

    ports.sort_by_key(|p| p.port);
    ports
}

/// Parse `netstat -tlnp` output (fallback if ss unavailable).
pub fn parse_netstat_output(output: &str) -> Vec<DiscoveredPort> {
    let mut ports = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with("tcp") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 || parts[5] != "LISTEN" {
            continue;
        }

        let local_addr = parts[3];

        // netstat uses ":::8080" for IPv6 wildcard
        let (bind_address, port) = if local_addr.starts_with(":::") {
            ("::", local_addr[3..].parse::<u16>().ok())
        } else {
            parse_address_port(local_addr)
        };

        let port = match port {
            Some(p) => p,
            None => continue,
        };

        // PID/Program: "1234/python3" or "-"
        let (pid, process_name) = if parts.len() >= 7 && parts[6] != "-" {
            let pid_prog = parts[6];
            let mut split = pid_prog.splitn(2, '/');
            let pid = split.next().and_then(|s| s.parse().ok());
            let name = split.next().map(|s| s.to_string());
            (pid, name)
        } else {
            (None, None)
        };

        ports.push(DiscoveredPort {
            port,
            bind_address: bind_address.to_string(),
            process_name,
            pid,
        });
    }

    ports.sort_by_key(|p| p.port);
    ports
}

fn parse_address_port(addr: &str) -> (&str, Option<u16>) {
    // Handle IPv6 bracket notation: [::]:8080
    if let Some(bracket_end) = addr.rfind("]:") {
        let host = &addr[..bracket_end + 1];
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let port = addr[bracket_end + 2..].parse().ok();
        return (host, port);
    }

    // Handle IPv4: 0.0.0.0:18080
    if let Some(colon_pos) = addr.rfind(':') {
        let host = &addr[..colon_pos];
        let port = addr[colon_pos + 1..].parse().ok();
        return (host, port);
    }

    (addr, None)
}

fn parse_ss_process_info(info: &str) -> (Option<String>, Option<u32>) {
    // Format: users:(("python3",pid=1234,fd=5))
    let name = info.split('"').nth(1).map(|s| s.to_string());

    let pid = info
        .split("pid=")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|s| s.parse().ok());

    (name, pid)
}

/// Parse `lsof -iTCP -sTCP:LISTEN -nP` output (macOS).
pub fn parse_lsof_output(output: &str) -> Vec<DiscoveredPort> {
    let mut ports = Vec::new();

    for line in output.lines().skip(1) {
        let line = line.trim();
        if !line.contains("(LISTEN)") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let process_name = Some(parts[0].to_string());
        let pid = parts[1].parse::<u32>().ok();

        // NAME column is second-to-last, e.g. "127.0.0.1:3000" or "[::]:80" or "*:3000"
        let name_raw = parts[parts.len() - 2];

        // Handle wildcard "*:port" → "0.0.0.0:port"
        let name_owned;
        let name = if name_raw.starts_with('*') {
            name_owned = name_raw.replacen('*', "0.0.0.0", 1);
            &name_owned
        } else {
            name_raw
        };

        let (bind_address, port) = parse_address_port(name);

        let port = match port {
            Some(p) => p,
            None => continue,
        };

        ports.push(DiscoveredPort {
            port,
            bind_address: bind_address.to_string(),
            process_name,
            pid,
        });
    }

    ports.sort_by_key(|p| p.port);
    ports
}

/// Deduplicate ports by port number for remote discovery.
/// When the same port appears multiple times (e.g. IPv4 + IPv6),
/// keeps the entry with the most information.
fn dedup_by_port(ports: Vec<DiscoveredPort>) -> Vec<DiscoveredPort> {
    let mut best: HashMap<u16, DiscoveredPort> = HashMap::new();
    for p in ports {
        if let Some(existing) = best.get(&p.port) {
            if !should_replace(existing, &p) {
                continue;
            }
        }
        best.insert(p.port, p);
    }
    let mut result: Vec<_> = best.into_values().collect();
    result.sort_by_key(|p| p.port);
    result
}

fn should_replace(existing: &DiscoveredPort, candidate: &DiscoveredPort) -> bool {
    if candidate.process_name.is_some() && existing.process_name.is_none() {
        return true;
    }
    if candidate.process_name.is_none() && existing.process_name.is_some() {
        return false;
    }
    is_wildcard(&candidate.bind_address) && !is_wildcard(&existing.bind_address)
}

fn is_wildcard(addr: &str) -> bool {
    matches!(addr, "0.0.0.0" | "::" | "*")
}

/// Discover listening ports on the remote host via SSH.
/// Tries `ss -tlnp` first, falls back to `netstat -tlnp`.
/// Deduplicates entries that differ only by address family (IPv4/IPv6).
pub async fn discover_remote_ports(session: &SshSession) -> Result<Vec<DiscoveredPort>> {
    // Try ss first
    let output = session.exec("ss -tlnp 2>/dev/null").await?;
    if !output.is_empty() && output.lines().count() > 1 {
        return Ok(dedup_by_port(parse_ss_output(&output)));
    }

    // Fall back to netstat
    let output = session.exec("netstat -tlnp 2>/dev/null").await?;
    Ok(dedup_by_port(parse_netstat_output(&output)))
}

/// Discover listening ports on the local machine.
/// Uses `lsof` on macOS, `ss`/`netstat` on Linux.
pub async fn discover_local_ports() -> Vec<DiscoveredPort> {
    match std::env::consts::OS {
        "macos" => discover_local_ports_macos().await,
        _ => discover_local_ports_linux().await,
    }
}

async fn discover_local_ports_macos() -> Vec<DiscoveredPort> {
    let output = Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-nP"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_lsof_output(&stdout)
        }
        _ => Vec::new(),
    }
}

async fn discover_local_ports_linux() -> Vec<DiscoveredPort> {
    // Try ss first
    let output = Command::new("ss").args(["-tlnp"]).output().await;

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.lines().count() > 1 {
                return parse_ss_output(&stdout);
            }
        }
    }

    // Fall back to netstat
    let output = Command::new("netstat").args(["-tlnp"]).output().await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_netstat_output(&stdout)
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ss_basic() {
        let output = "\
State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
LISTEN 0      128          0.0.0.0:18080      0.0.0.0:*    users:((\"python3\",pid=1234,fd=5))
LISTEN 0      128          0.0.0.0:18120      0.0.0.0:*    users:((\"go-build\",pid=5678,fd=3))
LISTEN 0      128        127.0.0.1:6379       0.0.0.0:*    users:((\"redis\",pid=910,fd=6))
";
        let ports = parse_ss_output(output);
        assert_eq!(ports.len(), 3);
        // Sorted by port
        assert_eq!(ports[0].port, 6379);
        assert_eq!(ports[0].bind_address, "127.0.0.1");
        assert_eq!(ports[0].process_name.as_deref(), Some("redis"));
        assert_eq!(ports[1].port, 18080);
        assert_eq!(ports[1].bind_address, "0.0.0.0");
        assert_eq!(ports[1].process_name.as_deref(), Some("python3"));
        assert_eq!(ports[1].pid, Some(1234));
        assert_eq!(ports[2].port, 18120);
        assert_eq!(ports[2].process_name.as_deref(), Some("go-build"));
    }

    #[test]
    fn test_parse_ss_ipv6() {
        let output = "\
State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
LISTEN 0      128             [::]:8080          [::]:*    users:((\"nginx\",pid=100,fd=7))
LISTEN 0      128            [::1]:5432          [::]:*    users:((\"postgres\",pid=200,fd=4))
";
        let ports = parse_ss_output(output);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 5432);
        assert_eq!(ports[0].bind_address, "::1");
        assert_eq!(ports[1].port, 8080);
        assert_eq!(ports[1].bind_address, "::");
        assert_eq!(ports[1].process_name.as_deref(), Some("nginx"));
    }

    #[test]
    fn test_parse_ss_no_process_info() {
        let output = "\
State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
LISTEN 0      128          0.0.0.0:3000       0.0.0.0:*
";
        let ports = parse_ss_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 3000);
        assert_eq!(ports[0].process_name, None);
        assert_eq!(ports[0].pid, None);
    }

    #[test]
    fn test_parse_ss_empty() {
        let output = "State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process\n";
        let ports = parse_ss_output(output);
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_netstat_basic() {
        let output = "\
Active Internet connections (only servers)
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
tcp        0      0 0.0.0.0:18080           0.0.0.0:*               LISTEN      1234/python3
tcp        0      0 127.0.0.1:6379          0.0.0.0:*               LISTEN      910/redis
tcp6       0      0 :::8080                 :::*                    LISTEN      100/nginx
";
        let ports = parse_netstat_output(output);
        assert_eq!(ports.len(), 3);
        // Sorted by port
        assert_eq!(ports[0].port, 6379);
        assert_eq!(ports[0].bind_address, "127.0.0.1");
        assert_eq!(ports[1].port, 8080);
        assert_eq!(ports[1].bind_address, "::");
        assert_eq!(ports[2].port, 18080);
        assert_eq!(ports[2].process_name.as_deref(), Some("python3"));
        assert_eq!(ports[2].pid, Some(1234));
    }

    // ---- Display trait ----

    #[test]
    fn test_display_with_process_name() {
        let port = DiscoveredPort {
            port: 8080,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some("nginx".to_string()),
            pid: Some(100),
        };
        assert_eq!(format!("{}", port), "0.0.0.0:8080 (nginx)");
    }

    #[test]
    fn test_display_without_process_name() {
        let port = DiscoveredPort {
            port: 3000,
            bind_address: "::".to_string(),
            process_name: None,
            pid: None,
        };
        assert_eq!(format!("{}", port), ":::3000 (unknown)");
    }

    // ---- parse_address_port direct tests ----

    #[test]
    fn test_parse_address_port_ipv4() {
        let (host, port) = parse_address_port("0.0.0.0:8080");
        assert_eq!(host, "0.0.0.0");
        assert_eq!(port, Some(8080));
    }

    #[test]
    fn test_parse_address_port_ipv6_bracket() {
        let (host, port) = parse_address_port("[::]:443");
        assert_eq!(host, "::");
        assert_eq!(port, Some(443));
    }

    #[test]
    fn test_parse_address_port_ipv6_localhost() {
        let (host, port) = parse_address_port("[::1]:5432");
        assert_eq!(host, "::1");
        assert_eq!(port, Some(5432));
    }

    #[test]
    fn test_parse_address_port_no_colon() {
        let (host, port) = parse_address_port("somehost");
        assert_eq!(host, "somehost");
        assert_eq!(port, None);
    }

    #[test]
    fn test_parse_address_port_invalid_port() {
        let (host, port) = parse_address_port("0.0.0.0:notaport");
        assert_eq!(host, "0.0.0.0");
        assert_eq!(port, None);
    }

    // ---- parse_ss_process_info direct tests ----

    #[test]
    fn test_parse_ss_process_info_normal() {
        let (name, pid) = parse_ss_process_info("users:((\"python3\",pid=1234,fd=5))");
        assert_eq!(name.as_deref(), Some("python3"));
        assert_eq!(pid, Some(1234));
    }

    #[test]
    fn test_parse_ss_process_info_no_quotes() {
        let (name, pid) = parse_ss_process_info("users:((pid=999))");
        assert_eq!(name, None);
        assert_eq!(pid, Some(999));
    }

    #[test]
    fn test_parse_ss_process_info_empty() {
        let (name, pid) = parse_ss_process_info("");
        assert_eq!(name, None);
        assert_eq!(pid, None);
    }

    // ---- ss output edge cases ----

    #[test]
    fn test_parse_ss_skips_non_listen_lines() {
        let output = "\
State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
ESTAB  0      128          0.0.0.0:8080       10.0.0.1:4321
TIME-WAIT 0   0           0.0.0.0:9090       10.0.0.2:5555
LISTEN 0      128          0.0.0.0:3000       0.0.0.0:*
";
        let ports = parse_ss_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 3000);
    }

    #[test]
    fn test_parse_ss_mixed_ipv4_and_ipv6() {
        let output = "\
State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
LISTEN 0      128          0.0.0.0:8080      0.0.0.0:*    users:((\"web\",pid=1,fd=3))
LISTEN 0      128             [::]:8080         [::]:*    users:((\"web\",pid=1,fd=4))
LISTEN 0      128        127.0.0.1:9090      0.0.0.0:*    users:((\"local\",pid=2,fd=5))
LISTEN 0      128            [::1]:9090         [::]:*    users:((\"local\",pid=2,fd=6))
";
        let ports = parse_ss_output(output);
        assert_eq!(ports.len(), 4);
        // Sorted by port: 8080 (x2), 9090 (x2)
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].bind_address, "0.0.0.0");
        assert_eq!(ports[1].port, 8080);
        assert_eq!(ports[1].bind_address, "::");
        assert_eq!(ports[2].port, 9090);
        assert_eq!(ports[2].bind_address, "127.0.0.1");
        assert_eq!(ports[3].port, 9090);
        assert_eq!(ports[3].bind_address, "::1");
    }

    // ---- netstat edge cases ----

    #[test]
    fn test_parse_netstat_no_pid() {
        let output = "\
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
tcp        0      0 0.0.0.0:80              0.0.0.0:*               LISTEN      -
";
        let ports = parse_netstat_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 80);
        assert_eq!(ports[0].process_name, None);
        assert_eq!(ports[0].pid, None);
    }

    #[test]
    fn test_parse_netstat_includes_localhost() {
        let output = "\
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
tcp        0      0 127.0.0.1:3306          0.0.0.0:*               LISTEN      500/mysqld
";
        let ports = parse_netstat_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 3306);
        assert_eq!(ports[0].bind_address, "127.0.0.1");
    }

    #[test]
    fn test_parse_netstat_skips_non_listen() {
        let output = "\
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
tcp        0      0 0.0.0.0:80              10.0.0.1:4321           ESTABLISHED 100/nginx
tcp        0      0 0.0.0.0:443             0.0.0.0:*               LISTEN      100/nginx
";
        let ports = parse_netstat_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 443);
    }

    #[test]
    fn test_parse_netstat_empty() {
        let output = "\
Active Internet connections (only servers)
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
";
        let ports = parse_netstat_output(output);
        assert!(ports.is_empty());
    }

    // ---- lsof parser tests ----

    #[test]
    fn test_parse_lsof_basic() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     1234  user   23u IPv4   0x1234 0t0      TCP  127.0.0.1:3000 (LISTEN)
nginx     567  root    8u IPv6   0x5678 0t0      TCP  [::]:80 (LISTEN)
python3  8901  user   10u IPv4   0xabcd 0t0      TCP  0.0.0.0:8080 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 3);
        // Sorted by port
        assert_eq!(ports[0].port, 80);
        assert_eq!(ports[0].bind_address, "::");
        assert_eq!(ports[0].process_name.as_deref(), Some("nginx"));
        assert_eq!(ports[0].pid, Some(567));
        assert_eq!(ports[1].port, 3000);
        assert_eq!(ports[1].bind_address, "127.0.0.1");
        assert_eq!(ports[1].process_name.as_deref(), Some("node"));
        assert_eq!(ports[1].pid, Some(1234));
        assert_eq!(ports[2].port, 8080);
        assert_eq!(ports[2].bind_address, "0.0.0.0");
        assert_eq!(ports[2].process_name.as_deref(), Some("python3"));
        assert_eq!(ports[2].pid, Some(8901));
    }

    #[test]
    fn test_parse_lsof_ipv6_localhost() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
postgres  200  user    4u IPv6   0xaaaa 0t0      TCP  [::1]:5432 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 5432);
        assert_eq!(ports[0].bind_address, "::1");
        assert_eq!(ports[0].process_name.as_deref(), Some("postgres"));
        assert_eq!(ports[0].pid, Some(200));
    }

    #[test]
    fn test_parse_lsof_wildcard_star() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     1234  user   23u IPv4   0x1234 0t0      TCP  *:3000 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 3000);
        assert_eq!(ports[0].bind_address, "0.0.0.0");
    }

    #[test]
    fn test_parse_lsof_skips_non_listen() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     1234  user   23u IPv4   0x1234 0t0      TCP  127.0.0.1:3000 (ESTABLISHED)
nginx     567  root    8u IPv4   0x5678 0t0      TCP  0.0.0.0:80 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 80);
    }

    #[test]
    fn test_parse_lsof_empty() {
        let output = "COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME\n";
        let ports = parse_lsof_output(output);
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_lsof_no_output() {
        let ports = parse_lsof_output("");
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_lsof_malformed_line() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
this is garbage
node     1234  user   23u IPv4   0x1234 0t0      TCP  0.0.0.0:8080 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 8080);
    }

    #[test]
    fn test_parse_lsof_invalid_port() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     1234  user   23u IPv4   0x1234 0t0      TCP  0.0.0.0:notaport (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert!(ports.is_empty());
    }

    #[test]
    fn test_parse_lsof_invalid_pid() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     notapid  user   23u IPv4   0x1234 0t0      TCP  0.0.0.0:8080 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].pid, None);
        assert_eq!(ports[0].process_name.as_deref(), Some("node"));
    }

    #[test]
    fn test_parse_lsof_duplicate_ports_different_addresses() {
        let output = "\
COMMAND   PID  USER   FD  TYPE   DEVICE SIZE/OFF NODE NAME
node     1234  user   23u IPv4   0x1234 0t0      TCP  127.0.0.1:3000 (LISTEN)
node     1234  user   24u IPv4   0x1235 0t0      TCP  0.0.0.0:3000 (LISTEN)
";
        let ports = parse_lsof_output(output);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 3000);
        assert_eq!(ports[1].port, 3000);
    }

    // ---- discover_local_ports integration test ----

    #[tokio::test]
    async fn test_discover_local_ports_finds_something() {
        // The test runner itself (or OS services) will have listening ports
        let ports = discover_local_ports().await;
        assert!(
            !ports.is_empty(),
            "Expected at least one listening port on localhost"
        );
        for p in &ports {
            assert!(p.port > 0);
            assert!(!p.bind_address.is_empty());
        }
    }

    // ---- dedup_by_port tests ----

    #[test]
    fn test_dedup_ipv4_and_ipv6_same_port() {
        let ports = vec![
            DiscoveredPort {
                port: 22,
                bind_address: "0.0.0.0".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 22,
                bind_address: "::".to_string(),
                process_name: None,
                pid: None,
            },
        ];
        let result = dedup_by_port(ports);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].port, 22);
    }

    #[test]
    fn test_dedup_prefers_entry_with_process_info() {
        let ports = vec![
            DiscoveredPort {
                port: 53,
                bind_address: "0.0.0.0".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 53,
                bind_address: "::".to_string(),
                process_name: Some("named".to_string()),
                pid: Some(100),
            },
        ];
        let result = dedup_by_port(ports);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].process_name.as_deref(), Some("named"));
        assert_eq!(result[0].pid, Some(100));
    }

    #[test]
    fn test_dedup_prefers_wildcard_address() {
        let ports = vec![
            DiscoveredPort {
                port: 8080,
                bind_address: "127.0.0.1".to_string(),
                process_name: Some("nginx".to_string()),
                pid: Some(1),
            },
            DiscoveredPort {
                port: 8080,
                bind_address: "0.0.0.0".to_string(),
                process_name: Some("nginx".to_string()),
                pid: Some(1),
            },
        ];
        let result = dedup_by_port(ports);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].bind_address, "0.0.0.0");
    }

    #[test]
    fn test_dedup_no_duplicates() {
        let ports = vec![
            DiscoveredPort {
                port: 22,
                bind_address: "0.0.0.0".to_string(),
                process_name: Some("sshd".to_string()),
                pid: Some(1),
            },
            DiscoveredPort {
                port: 80,
                bind_address: "0.0.0.0".to_string(),
                process_name: Some("nginx".to_string()),
                pid: Some(2),
            },
        ];
        let result = dedup_by_port(ports);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].port, 22);
        assert_eq!(result[1].port, 80);
    }

    #[test]
    fn test_dedup_empty() {
        let result = dedup_by_port(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_dedup_multiple_ports_each_duplicated() {
        let ports = vec![
            DiscoveredPort {
                port: 22,
                bind_address: "0.0.0.0".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 22,
                bind_address: "::".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 53,
                bind_address: "0.0.0.0".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 53,
                bind_address: "::".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 8080,
                bind_address: "0.0.0.0".to_string(),
                process_name: Some("nginx".to_string()),
                pid: Some(100),
            },
        ];
        let result = dedup_by_port(ports);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].port, 22);
        assert_eq!(result[1].port, 53);
        assert_eq!(result[2].port, 8080);
    }

    #[test]
    fn test_dedup_keeps_process_info_over_wildcard() {
        // localhost entry has process info, wildcard entry does not
        // should keep the one with process info
        let ports = vec![
            DiscoveredPort {
                port: 111,
                bind_address: "0.0.0.0".to_string(),
                process_name: None,
                pid: None,
            },
            DiscoveredPort {
                port: 111,
                bind_address: "127.0.0.1".to_string(),
                process_name: Some("rpcbind".to_string()),
                pid: Some(500),
            },
        ];
        let result = dedup_by_port(ports);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].process_name.as_deref(), Some("rpcbind"));
    }
}
