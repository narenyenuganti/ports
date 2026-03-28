# portfwd Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI/TUI tool that connects to a remote host via SSH, discovers listening ports, and lets the user selectively forward them locally.

**Architecture:** Single async binary with three layers — SSH transport (russh), core state machine, and TUI (ratatui). All communication multiplexed over one SSH session. Forwarding uses local TCP listeners that proxy through SSH direct-tcpip channels.

**Tech Stack:** Rust, russh, russh-keys, ssh2-config, ratatui, crossterm, tokio, clap, anyhow

---

## File Map

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | Dependencies and project metadata |
| `src/main.rs` | CLI entry point (clap), bootstrap async runtime, wire layers together |
| `src/ssh/mod.rs` | Re-exports ssh module |
| `src/ssh/config.rs` | Parse `~/.ssh/config`, extract host params |
| `src/ssh/connection.rs` | russh client handler, connect, authenticate, exec commands |
| `src/ssh/discovery.rs` | Parse `ss -tlnp` / `netstat -tlnp` output into `DiscoveredPort` structs |
| `src/forward/mod.rs` | Re-exports forward module |
| `src/forward/tunnel.rs` | Local TCP listener, SSH direct-tcpip proxy, forward lifecycle management |
| `src/tui/mod.rs` | Re-exports tui module |
| `src/tui/app.rs` | App state (ports, selection, connection status), state transitions, event loop |
| `src/tui/ui.rs` | ratatui rendering — table, status bar, help bar |
| `src/tui/input.rs` | Key event handling, inline text input for port override |

---

### Task 1: Project scaffolding & CLI

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/ssh/mod.rs`
- Create: `src/ssh/config.rs`
- Create: `src/ssh/connection.rs`
- Create: `src/ssh/discovery.rs`
- Create: `src/forward/mod.rs`
- Create: `src/forward/tunnel.rs`
- Create: `src/tui/mod.rs`
- Create: `src/tui/app.rs`
- Create: `src/tui/ui.rs`
- Create: `src/tui/input.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "portfwd"
version = "0.1.0"
edition = "2021"
description = "Lightweight SSH port forwarding TUI"

[dependencies]
russh = "0.46"
russh-keys = "0.46"
ssh2-config = "0.4"
ratatui = "0.29"
crossterm = "0.28"
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
anyhow = "1"
async-trait = "0.1"
log = "0.4"
```

- [ ] **Step 2: Create src/main.rs with clap CLI**

```rust
mod ssh;
mod forward;
mod tui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "portfwd", about = "Lightweight SSH port forwarding TUI")]
struct Cli {
    /// SSH config host alias to connect to
    host: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    println!("Connecting to {}...", cli.host);
    Ok(())
}
```

- [ ] **Step 3: Create module stub files**

`src/ssh/mod.rs`:
```rust
pub mod config;
pub mod connection;
pub mod discovery;
```

`src/ssh/config.rs`:
```rust
// SSH config parsing
```

`src/ssh/connection.rs`:
```rust
// SSH connection management
```

`src/ssh/discovery.rs`:
```rust
// Remote port discovery
```

`src/forward/mod.rs`:
```rust
pub mod tunnel;
```

`src/forward/tunnel.rs`:
```rust
// Port forwarding tunnel
```

`src/tui/mod.rs`:
```rust
pub mod app;
pub mod ui;
pub mod input;
```

`src/tui/app.rs`:
```rust
// App state machine
```

`src/tui/ui.rs`:
```rust
// TUI rendering
```

`src/tui/input.rs`:
```rust
// Key event handling
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles with warnings about unused modules (OK)

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "feat: scaffold project structure with CLI entry point"
```

---

### Task 2: Port discovery parser (TDD)

**Files:**
- Modify: `src/ssh/discovery.rs`

- [ ] **Step 1: Write the data model and failing tests**

```rust
use std::fmt;

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
    todo!()
}

/// Parse `netstat -tlnp` output (fallback if ss unavailable).
pub fn parse_netstat_output(output: &str) -> Vec<DiscoveredPort> {
    todo!()
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ssh::discovery`
Expected: FAIL — `todo!()` panics

- [ ] **Step 3: Implement parse_ss_output**

Replace the `todo!()` in `parse_ss_output` with:

```rust
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

        // Filter: only 0.0.0.0 or :: (not 127.0.0.1 or ::1)
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

fn parse_address_port(addr: &str) -> (&str, Option<u16>) {
    // Handle IPv6 bracket notation: [::]:8080
    if let Some(bracket_end) = addr.rfind("]:") {
        let host = &addr[..bracket_end + 1];
        // Strip brackets for storage
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
    let name = info
        .split('"')
        .nth(1)
        .map(|s| s.to_string());

    let pid = info
        .split("pid=")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|s| s.parse().ok());

    (name, pid)
}
```

- [ ] **Step 4: Implement parse_netstat_output**

Replace the `todo!()` in `parse_netstat_output` with:

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib ssh::discovery`
Expected: All 5 tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/ssh/discovery.rs
git commit -m "feat: implement ss/netstat output parsers with tests"
```

---

### Task 3: SSH config parsing

**Files:**
- Modify: `src/ssh/config.rs`

- [ ] **Step 1: Implement SSH config wrapper**

```rust
use anyhow::{Context, Result};
use ssh2_config::{ParseRule, SshConfig};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub hostname: String,
    pub user: String,
    pub port: u16,
    pub identity_files: Vec<PathBuf>,
}

pub fn load_host_config(host_alias: &str) -> Result<HostConfig> {
    let config_path = dirs_ssh_config_path();
    let config_str = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read SSH config at {}", config_path.display()))?;

    let mut config = SshConfig::default();
    config
        .parse(&mut config_str.as_bytes(), ParseRule::STRICT)
        .context("Failed to parse SSH config")?;

    let params = config.query(host_alias);

    let hostname = params
        .host_name
        .unwrap_or_else(|| host_alias.to_string());

    let user = params
        .user
        .unwrap_or_else(|| whoami::username());

    let port = params.port.unwrap_or(22);

    let identity_files = params
        .identity_file
        .unwrap_or_default()
        .into_iter()
        .map(|p| expand_tilde(&p))
        .collect();

    Ok(HostConfig {
        hostname,
        user,
        port,
        identity_files,
    })
}

fn dirs_ssh_config_path() -> PathBuf {
    let home = dirs_home();
    home.join(".ssh").join("config")
}

fn dirs_home() -> PathBuf {
    home::home_dir().expect("Could not determine home directory")
}

fn expand_tilde(path: &PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        dirs_home().join(&s[2..])
    } else {
        path.clone()
    }
}
```

- [ ] **Step 2: Add home and whoami crates to Cargo.toml**

Add to `[dependencies]`:
```toml
home = "0.5"
whoami = "1"
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/ssh/config.rs
git commit -m "feat: implement SSH config parsing from ~/.ssh/config"
```

---

### Task 4: SSH connection handler

**Files:**
- Modify: `src/ssh/connection.rs`

- [ ] **Step 1: Implement russh client handler and connection logic**

```rust
use anyhow::{Context, Result};
use async_trait::async_trait;
use russh::keys::ssh_key::PrivateKey;
use russh::*;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::config::HostConfig;

pub struct SshSession {
    pub handle: client::Handle<ClientHandler>,
}

pub struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Accept all host keys (similar to ssh -o StrictHostKeyChecking=no)
        // Future improvement: check known_hosts
        Ok(true)
    }
}

impl SshSession {
    pub async fn connect(config: &HostConfig) -> Result<Self> {
        let ssh_config = client::Config {
            ..Default::default()
        };

        let handler = ClientHandler;

        let mut handle = client::connect(
            Arc::new(ssh_config),
            (config.hostname.as_str(), config.port),
            handler,
        )
        .await
        .with_context(|| format!("Failed to connect to {}:{}", config.hostname, config.port))?;

        // Try authentication methods in order
        let authenticated = try_agent_auth(&mut handle, &config.user).await
            || try_identity_files(&mut handle, &config.user, &config.identity_files).await
            || try_default_keys(&mut handle, &config.user).await;

        if !authenticated {
            anyhow::bail!(
                "Authentication failed for {}@{}:{}",
                config.user,
                config.hostname,
                config.port
            );
        }

        Ok(SshSession { handle })
    }

    /// Execute a command on the remote and return its stdout.
    pub async fn exec(&self, command: &str) -> Result<String> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .context("Failed to open SSH session channel")?;

        channel
            .exec(true, command)
            .await
            .context("Failed to execute remote command")?;

        let mut output = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { ref data } => {
                    output.extend_from_slice(data);
                }
                ChannelMsg::Eof => break,
                _ => {}
            }
        }

        String::from_utf8(output).context("Remote command output was not valid UTF-8")
    }

    /// Open a direct-tcpip channel for port forwarding.
    pub async fn open_direct_tcpip(
        &self,
        remote_host: &str,
        remote_port: u16,
        local_host: &str,
        local_port: u16,
    ) -> Result<Channel<client::Msg>> {
        self.handle
            .channel_open_direct_tcpip(remote_host, remote_port as u32, local_host, local_port as u32)
            .await
            .context("Failed to open direct-tcpip channel")
    }
}

async fn try_agent_auth(handle: &mut client::Handle<ClientHandler>, user: &str) -> bool {
    match russh_keys::agent::client::AgentClient::connect_env().await {
        Ok(mut agent) => {
            let identities = match agent.request_identities().await {
                Ok(ids) => ids,
                Err(_) => return false,
            };
            for key in identities {
                if handle
                    .authenticate_future(user, key, &mut agent)
                    .await
                    .unwrap_or((_, false))
                    .1
                {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

async fn try_identity_files(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
    identity_files: &[PathBuf],
) -> bool {
    for path in identity_files {
        if try_key_file(handle, user, path).await {
            return true;
        }
    }
    false
}

async fn try_default_keys(handle: &mut client::Handle<ClientHandler>, user: &str) -> bool {
    let home = home::home_dir().expect("Could not determine home directory");
    let default_keys = [
        home.join(".ssh").join("id_ed25519"),
        home.join(".ssh").join("id_rsa"),
    ];
    for path in &default_keys {
        if try_key_file(handle, user, path).await {
            return true;
        }
    }
    false
}

async fn try_key_file(
    handle: &mut client::Handle<ClientHandler>,
    user: &str,
    path: &PathBuf,
) -> bool {
    let key = match russh_keys::load_secret_key(path, None).await {
        Ok(k) => k,
        Err(_) => return false,
    };
    let key_pair = Arc::new(key);
    handle
        .authenticate_publickey(user, key_pair)
        .await
        .unwrap_or(false)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles (may need minor type adjustments based on exact russh version)

- [ ] **Step 3: Commit**

```bash
git add src/ssh/connection.rs
git commit -m "feat: implement SSH connection with agent/key auth"
```

---

### Task 5: Remote port discovery

**Files:**
- Modify: `src/ssh/discovery.rs`

- [ ] **Step 1: Add remote discovery function that uses SSH exec**

Add to the top of `src/ssh/discovery.rs`, before the existing code:

```rust
use anyhow::Result;
use super::connection::SshSession;
```

Add after the existing parsing functions (before `#[cfg(test)]`):

```rust
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
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src/ssh/discovery.rs
git commit -m "feat: add remote port discovery via SSH exec"
```

---

### Task 6: App state model (TDD)

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 1: Write data model, state, and failing tests**

```rust
use crate::ssh::discovery::DiscoveredPort;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardStatus {
    /// Port discovered but not forwarded
    Idle,
    /// Currently forwarded
    Active { local_port: u16 },
    /// Forward failed (e.g., port conflict)
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PortEntry {
    pub discovered: DiscoveredPort,
    pub forward_status: ForwardStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    /// User is typing a port number override
    PortInput(String),
}

pub struct AppState {
    pub host_alias: String,
    pub connection: ConnectionState,
    pub ports: Vec<PortEntry>,
    pub selected: usize,
    pub input_mode: InputMode,
    pub status_message: Option<String>,
}

impl AppState {
    pub fn new(host_alias: String) -> Self {
        Self {
            host_alias,
            connection: ConnectionState::Connected,
            ports: Vec::new(),
            selected: 0,
            input_mode: InputMode::Normal,
            status_message: None,
        }
    }

    pub fn update_ports(&mut self, discovered: Vec<DiscoveredPort>) {
        // Keep existing forward state for ports that are still present
        let mut new_ports = Vec::new();
        for dp in discovered {
            let existing = self
                .ports
                .iter()
                .find(|p| p.discovered.port == dp.port && p.discovered.bind_address == dp.bind_address);

            let forward_status = match existing {
                Some(e) => e.forward_status.clone(),
                None => ForwardStatus::Idle,
            };

            new_ports.push(PortEntry {
                discovered: dp,
                forward_status,
            });
        }
        self.ports = new_ports;
        // Clamp selection
        if self.selected >= self.ports.len() && !self.ports.is_empty() {
            self.selected = self.ports.len() - 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.ports.len() {
            self.selected += 1;
        }
    }

    pub fn selected_port(&self) -> Option<&PortEntry> {
        self.ports.get(self.selected)
    }

    pub fn set_forward_active(&mut self, index: usize, local_port: u16) {
        if let Some(entry) = self.ports.get_mut(index) {
            entry.forward_status = ForwardStatus::Active { local_port };
        }
    }

    pub fn set_forward_idle(&mut self, index: usize) {
        if let Some(entry) = self.ports.get_mut(index) {
            entry.forward_status = ForwardStatus::Idle;
        }
    }

    pub fn set_forward_error(&mut self, index: usize, msg: String) {
        if let Some(entry) = self.ports.get_mut(index) {
            entry.forward_status = ForwardStatus::Error(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_port(port: u16, name: &str) -> DiscoveredPort {
        DiscoveredPort {
            port,
            bind_address: "0.0.0.0".to_string(),
            process_name: Some(name.to_string()),
            pid: Some(1000),
        }
    }

    #[test]
    fn test_new_state() {
        let state = AppState::new("my-remote".to_string());
        assert_eq!(state.host_alias, "my-remote");
        assert_eq!(state.connection, ConnectionState::Connected);
        assert!(state.ports.is_empty());
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_update_ports_fresh() {
        let mut state = AppState::new("host".to_string());
        let ports = vec![make_port(8080, "nginx"), make_port(3000, "node")];
        state.update_ports(ports);
        assert_eq!(state.ports.len(), 2);
        assert_eq!(state.ports[0].forward_status, ForwardStatus::Idle);
        assert_eq!(state.ports[1].forward_status, ForwardStatus::Idle);
    }

    #[test]
    fn test_update_ports_preserves_forward_state() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "nginx"), make_port(3000, "node")]);
        state.set_forward_active(0, 8080);

        // Re-discover — 8080 still there, 3000 gone, 5000 new
        state.update_ports(vec![make_port(8080, "nginx"), make_port(5000, "python")]);
        assert_eq!(
            state.ports[0].forward_status,
            ForwardStatus::Active { local_port: 8080 }
        );
        assert_eq!(state.ports[1].forward_status, ForwardStatus::Idle);
    }

    #[test]
    fn test_navigation() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![
            make_port(8080, "a"),
            make_port(3000, "b"),
            make_port(5000, "c"),
        ]);

        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 1);
        state.move_down();
        assert_eq!(state.selected, 2);
        state.move_down(); // at bottom, stays
        assert_eq!(state.selected, 2);
        state.move_up();
        assert_eq!(state.selected, 1);
        state.move_up();
        assert_eq!(state.selected, 0);
        state.move_up(); // at top, stays
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_selection_clamp_on_update() {
        let mut state = AppState::new("host".to_string());
        state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
        state.selected = 1;
        // Shrink to 1 port
        state.update_ports(vec![make_port(8080, "a")]);
        assert_eq!(state.selected, 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib tui::app`
Expected: All 5 tests PASS

- [ ] **Step 3: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat: implement app state model with tests"
```

---

### Task 7: Port forwarding tunnel

**Files:**
- Modify: `src/forward/tunnel.rs`

- [ ] **Step 1: Implement the forward manager**

```rust
use anyhow::{Context, Result};
use russh::{Channel, client};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::ssh::connection::SshSession;

pub struct ForwardManager {
    session: Arc<SshSession>,
    active_forwards: HashMap<u16, CancellationToken>,
}

impl ForwardManager {
    pub fn new(session: Arc<SshSession>) -> Self {
        Self {
            session,
            active_forwards: HashMap::new(),
        }
    }

    /// Start forwarding: bind locally and proxy to remote via SSH.
    /// Returns the actual local port bound (may differ if user chose alternate).
    pub async fn start_forward(
        &mut self,
        remote_host: &str,
        remote_port: u16,
        local_port: u16,
    ) -> Result<u16> {
        let listener = TcpListener::bind(("127.0.0.1", local_port))
            .await
            .with_context(|| format!("Failed to bind local port {}", local_port))?;

        let actual_port = listener.local_addr()?.port();
        let token = CancellationToken::new();
        let child_token = token.clone();
        let session = self.session.clone();
        let r_host = remote_host.to_string();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = child_token.cancelled() => break,
                    accept = listener.accept() => {
                        match accept {
                            Ok((tcp_stream, _)) => {
                                let session = session.clone();
                                let r_host = r_host.clone();
                                let conn_token = child_token.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = handle_connection(
                                        tcp_stream,
                                        &session,
                                        &r_host,
                                        remote_port,
                                        actual_port,
                                        conn_token,
                                    ).await {
                                        log::warn!("Forward connection error: {}", e);
                                    }
                                });
                            }
                            Err(e) => {
                                log::warn!("Accept error on port {}: {}", actual_port, e);
                            }
                        }
                    }
                }
            }
        });

        self.active_forwards.insert(remote_port, token);
        Ok(actual_port)
    }

    /// Stop forwarding a remote port.
    pub fn stop_forward(&mut self, remote_port: u16) {
        if let Some(token) = self.active_forwards.remove(&remote_port) {
            token.cancel();
        }
    }

    /// Stop all active forwards.
    pub fn stop_all(&mut self) {
        for (_, token) in self.active_forwards.drain() {
            token.cancel();
        }
    }

    pub fn is_forwarding(&self, remote_port: u16) -> bool {
        self.active_forwards.contains_key(&remote_port)
    }
}

async fn handle_connection(
    mut tcp_stream: tokio::net::TcpStream,
    session: &SshSession,
    remote_host: &str,
    remote_port: u16,
    local_port: u16,
    token: CancellationToken,
) -> Result<()> {
    let mut channel = session
        .open_direct_tcpip(remote_host, remote_port, "127.0.0.1", local_port)
        .await?;

    let (mut tcp_read, mut tcp_write) = tcp_stream.split();

    let mut buf_from_tcp = vec![0u8; 8192];

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            result = tcp_read.read(&mut buf_from_tcp) => {
                match result {
                    Ok(0) => break, // TCP closed
                    Ok(n) => {
                        channel.data(&buf_from_tcp[..n]).await?;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(russh::ChannelMsg::Data { ref data }) => {
                        tcp_write.write_all(data).await?;
                    }
                    Some(russh::ChannelMsg::Eof) | None => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Add tokio-util to Cargo.toml**

Add to `[dependencies]`:
```toml
tokio-util = "0.7"
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/forward/tunnel.rs
git commit -m "feat: implement port forwarding tunnel with TCP proxy"
```

---

### Task 8: TUI rendering

**Files:**
- Modify: `src/tui/ui.rs`

- [ ] **Step 1: Implement the rendering function**

```rust
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use super::app::{AppState, ConnectionState, ForwardStatus, InputMode};

pub fn render(f: &mut Frame, state: &AppState) {
    let chunks = Layout::vertical([
        Constraint::Length(1),  // status bar
        Constraint::Min(5),    // port table
        Constraint::Length(2), // help bar
    ])
    .split(f.area());

    render_status_bar(f, state, chunks[0]);
    render_port_table(f, state, chunks[1]);
    render_help_bar(f, state, chunks[2]);
}

fn render_status_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let conn_str = match &state.connection {
        ConnectionState::Connected => ("connected", Color::Green),
        ConnectionState::Disconnected => ("disconnected", Color::Red),
        ConnectionState::Reconnecting => ("reconnecting...", Color::Yellow),
    };

    let line = Line::from(vec![
        Span::styled(" portfwd", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" — "),
        Span::raw(&state.host_alias),
        Span::raw(" ("),
        Span::styled(conn_str.0, Style::default().fg(conn_str.1)),
        Span::raw(")"),
        if let Some(ref msg) = state.status_message {
            Span::styled(format!("  {}", msg), Style::default().fg(Color::Yellow))
        } else {
            Span::raw("")
        },
    ]);

    f.render_widget(Paragraph::new(line), area);
}

fn render_port_table(f: &mut Frame, state: &AppState, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Status"),
        Cell::from("Port"),
        Cell::from("Local Address"),
        Cell::from("Process"),
        Cell::from("PID"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = state
        .ports
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let (status_icon, local_addr) = match &entry.forward_status {
                ForwardStatus::Active { local_port } => {
                    (
                        Span::styled("●", Style::default().fg(Color::Green)),
                        format!("localhost:{}", local_port),
                    )
                }
                ForwardStatus::Idle => {
                    (Span::styled("○", Style::default().fg(Color::DarkGray)), String::new())
                }
                ForwardStatus::Error(msg) => {
                    (
                        Span::styled("✗", Style::default().fg(Color::Red)),
                        msg.clone(),
                    )
                }
            };

            let process = entry
                .discovered
                .process_name
                .as_deref()
                .unwrap_or("-");
            let pid = entry
                .discovered
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());

            let style = if i == state.selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(status_icon),
                Cell::from(entry.discovered.port.to_string()),
                Cell::from(local_addr),
                Cell::from(process.to_string()),
                Cell::from(pid),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(20),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Ports "));

    f.render_widget(table, area);
}

fn render_help_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let help_text = match &state.input_mode {
        InputMode::Normal => {
            Line::from(vec![
                Span::styled("[enter]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" toggle  "),
                Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" refresh  "),
                Span::styled("[p]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" change port  "),
                Span::styled("[c]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" reconnect  "),
                Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" quit"),
            ])
        }
        InputMode::PortInput(input) => {
            Line::from(vec![
                Span::raw(" Local port: "),
                Span::styled(input, Style::default().add_modifier(Modifier::BOLD)),
                Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
                Span::raw("  [enter] confirm  [esc] cancel"),
            ])
        }
    };

    f.render_widget(
        Paragraph::new(help_text).block(Block::default().borders(Borders::TOP)),
        area,
    );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: implement TUI rendering with port table and status bar"
```

---

### Task 9: TUI input handling

**Files:**
- Modify: `src/tui/input.rs`

- [ ] **Step 1: Implement key event handling**

```rust
use crossterm::event::{KeyCode, KeyEvent};

use super::app::{AppState, InputMode};

/// Actions the event loop should perform after handling input.
#[derive(Debug)]
pub enum Action {
    None,
    Quit,
    ToggleForward(usize),
    StartForwardWithPort(usize, u16),
    Refresh,
    Reconnect,
}

pub fn handle_key(state: &mut AppState, key: KeyEvent) -> Action {
    match &state.input_mode {
        InputMode::Normal => handle_normal_mode(state, key),
        InputMode::PortInput(_) => handle_port_input(state, key),
    }
}

fn handle_normal_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Up | KeyCode::Char('k') => {
            state.move_up();
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.move_down();
            Action::None
        }
        KeyCode::Enter => {
            if state.ports.is_empty() {
                return Action::None;
            }
            Action::ToggleForward(state.selected)
        }
        KeyCode::Char('r') => Action::Refresh,
        KeyCode::Char('c') => Action::Reconnect,
        KeyCode::Char('p') => {
            if !state.ports.is_empty() {
                state.input_mode = InputMode::PortInput(String::new());
            }
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_port_input(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Enter => {
            if let InputMode::PortInput(ref input) = state.input_mode {
                let port_str = input.clone();
                state.input_mode = InputMode::Normal;
                if let Ok(port) = port_str.parse::<u16>() {
                    return Action::StartForwardWithPort(state.selected, port);
                } else {
                    state.status_message = Some("Invalid port number".to_string());
                }
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let InputMode::PortInput(ref mut input) = state.input_mode {
                input.pop();
            }
            Action::None
        }
        KeyCode::Char(c) if c.is_ascii_digit() => {
            if let InputMode::PortInput(ref mut input) = state.input_mode {
                input.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully

- [ ] **Step 3: Commit**

```bash
git add src/tui/input.rs
git commit -m "feat: implement TUI key event handling"
```

---

### Task 10: Main event loop integration

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Wire everything together in main.rs**

```rust
mod forward;
mod ssh;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use std::io::stdout;
use std::sync::Arc;
use std::time::Duration;

use forward::tunnel::ForwardManager;
use ssh::config::load_host_config;
use ssh::connection::SshSession;
use ssh::discovery::discover_remote_ports;
use tui::app::{AppState, ConnectionState, ForwardStatus};
use tui::input::{handle_key, Action};
use tui::ui::render;

#[derive(Parser)]
#[command(name = "portfwd", about = "Lightweight SSH port forwarding TUI")]
struct Cli {
    /// SSH config host alias to connect to
    host: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Parse SSH config
    let host_config = load_host_config(&cli.host)
        .with_context(|| format!("Failed to load SSH config for host '{}'", cli.host))?;

    eprintln!(
        "Connecting to {}@{}:{}...",
        host_config.user, host_config.hostname, host_config.port
    );

    // Connect
    let session = SshSession::connect(&host_config).await?;
    let session = Arc::new(session);

    // Discover ports
    let discovered = discover_remote_ports(&session).await?;

    // Initialize app state
    let mut state = AppState::new(cli.host.clone());
    state.update_ports(discovered);

    // Initialize forward manager
    let mut fwd_manager = ForwardManager::new(session.clone());

    // Set up terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    // Main event loop
    let result = run_loop(&mut terminal, &mut state, &mut fwd_manager, &session).await;

    // Cleanup
    fwd_manager.stop_all();
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut AppState,
    fwd_manager: &mut ForwardManager,
    session: &Arc<SshSession>,
) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, state))?;

        // Poll for events with a 100ms timeout so the UI stays responsive
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Only handle key press events (not release/repeat)
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                let action = handle_key(state, key);
                match action {
                    Action::Quit => break,
                    Action::ToggleForward(idx) => {
                        toggle_forward(state, fwd_manager, idx).await;
                    }
                    Action::StartForwardWithPort(idx, local_port) => {
                        start_forward_with_port(state, fwd_manager, idx, local_port).await;
                    }
                    Action::Refresh => {
                        state.status_message = Some("Scanning...".to_string());
                        terminal.draw(|f| render(f, state))?;
                        match discover_remote_ports(session).await {
                            Ok(ports) => {
                                state.update_ports(ports);
                                state.status_message = None;
                            }
                            Err(e) => {
                                state.status_message =
                                    Some(format!("Scan failed: {}", e));
                            }
                        }
                    }
                    Action::Reconnect => {
                        state.status_message =
                            Some("Reconnect not yet implemented".to_string());
                    }
                    Action::None => {}
                }
            }
        }
    }

    Ok(())
}

async fn toggle_forward(
    state: &mut AppState,
    fwd_manager: &mut ForwardManager,
    idx: usize,
) {
    let entry = match state.ports.get(idx) {
        Some(e) => e.clone(),
        None => return,
    };

    match &entry.forward_status {
        ForwardStatus::Active { .. } => {
            fwd_manager.stop_forward(entry.discovered.port);
            state.set_forward_idle(idx);
            state.status_message = Some(format!("Stopped forward for port {}", entry.discovered.port));
        }
        ForwardStatus::Idle | ForwardStatus::Error(_) => {
            let local_port = entry.discovered.port;
            match fwd_manager
                .start_forward("127.0.0.1", entry.discovered.port, local_port)
                .await
            {
                Ok(actual_port) => {
                    state.set_forward_active(idx, actual_port);
                    state.status_message = Some(format!(
                        "Forwarding localhost:{} → remote:{}",
                        actual_port, entry.discovered.port
                    ));
                }
                Err(e) => {
                    state.set_forward_error(idx, format!("{}", e));
                    state.status_message =
                        Some(format!("Failed to forward port {}: {}", local_port, e));
                }
            }
        }
    }
}

async fn start_forward_with_port(
    state: &mut AppState,
    fwd_manager: &mut ForwardManager,
    idx: usize,
    local_port: u16,
) {
    let entry = match state.ports.get(idx) {
        Some(e) => e.clone(),
        None => return,
    };

    // Stop existing forward if any
    if matches!(entry.forward_status, ForwardStatus::Active { .. }) {
        fwd_manager.stop_forward(entry.discovered.port);
    }

    match fwd_manager
        .start_forward("127.0.0.1", entry.discovered.port, local_port)
        .await
    {
        Ok(actual_port) => {
            state.set_forward_active(idx, actual_port);
            state.status_message = Some(format!(
                "Forwarding localhost:{} → remote:{}",
                actual_port, entry.discovered.port
            ));
        }
        Err(e) => {
            state.set_forward_error(idx, format!("{}", e));
            state.status_message = Some(format!("Failed to forward port {}: {}", local_port, e));
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles (may need minor adjustments)

- [ ] **Step 3: Manual smoke test**

Run: `cargo run -- <your-ssh-host>`
Expected: Connects, shows port table, keybinds work

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up main event loop connecting SSH, TUI, and forwarding"
```

---

## Post-Implementation

After all tasks complete, do a final `cargo clippy` pass and fix any warnings, then commit. Run `cargo build --release` to verify the release binary builds.
