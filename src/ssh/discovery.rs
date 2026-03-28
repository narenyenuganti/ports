use anyhow::Result;
use std::fmt;

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

        if bind_address == "127.0.0.1" || bind_address == "::1" {
            continue;
        }

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

        if bind_address == "127.0.0.1" || bind_address == "::1" {
            continue;
        }

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

/// Discover listening ports on the remote host via SSH.
/// Tries `ss -tlnp` first, falls back to `netstat -tlnp`.
pub async fn discover_remote_ports(session: &SshSession) -> Result<Vec<DiscoveredPort>> {
    // Try ss first
    let output = session.exec("ss -tlnp 2>/dev/null").await?;
    if !output.is_empty() && output.lines().count() > 1 {
        return Ok(parse_ss_output(&output));
    }

    // Fall back to netstat
    let output = session.exec("netstat -tlnp 2>/dev/null").await?;
    Ok(parse_netstat_output(&output))
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
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 18080);
        assert_eq!(ports[0].bind_address, "0.0.0.0");
        assert_eq!(ports[0].process_name.as_deref(), Some("python3"));
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 18120);
        assert_eq!(ports[1].process_name.as_deref(), Some("go-build"));
    }

    #[test]
    fn test_parse_ss_ipv6() {
        let output = "\
State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process
LISTEN 0      128             [::]:8080          [::]:*    users:((\"nginx\",pid=100,fd=7))
LISTEN 0      128            [::1]:5432          [::]:*    users:((\"postgres\",pid=200,fd=4))
";
        let ports = parse_ss_output(output);
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].bind_address, "::");
        assert_eq!(ports[0].process_name.as_deref(), Some("nginx"));
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
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 18080);
        assert_eq!(ports[0].process_name.as_deref(), Some("python3"));
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 8080);
        assert_eq!(ports[1].bind_address, "::");
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
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].bind_address, "0.0.0.0");
        assert_eq!(ports[1].bind_address, "::");
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
    fn test_parse_netstat_filters_localhost() {
        let output = "\
Proto Recv-Q Send-Q Local Address           Foreign Address         State       PID/Program name
tcp        0      0 127.0.0.1:3306          0.0.0.0:*               LISTEN      500/mysqld
";
        let ports = parse_netstat_output(output);
        assert!(ports.is_empty());
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
}
