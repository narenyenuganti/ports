# Local Server Detection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only "Local" view to portfwd's TUI showing listening ports on the local machine, toggled via Tab.

**Architecture:** Platform-adaptive local port discovery (`lsof` on macOS, `ss`/`netstat` on Linux) producing the same `DiscoveredPort` structs. A `ViewMode` enum in `AppState` drives view-specific rendering, navigation, and input handling. No reverse forwarding — purely informational.

**Tech Stack:** Rust, tokio (process::Command for local exec), ratatui, crossterm. No new crate dependencies.

---

## File Structure

| File | Responsibility | Change Type |
|------|---------------|-------------|
| `src/ssh/discovery.rs` | `parse_lsof_output()` parser, `discover_local_ports()` async fn | Modify |
| `src/tui/app.rs` | `ViewMode` enum, `local_ports`/`local_selected` fields, helper methods | Modify |
| `src/tui/input.rs` | Tab handling, view-mode-aware navigation/action gating | Modify |
| `src/tui/ui.rs` | View-mode-aware table, status bar label, help bar variants | Modify |
| `src/main.rs` | Concurrent startup discovery, view-mode-aware refresh | Modify |

---

### Task 1: `parse_lsof_output()` Parser

**Files:**
- Modify: `src/ssh/discovery.rs`

- [ ] **Step 1: Write failing tests for `parse_lsof_output()`**

Add these tests to the existing `#[cfg(test)] mod tests` block at the bottom of `src/ssh/discovery.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib ssh::discovery::tests::test_parse_lsof 2>&1 | head -30`
Expected: Compilation error — `parse_lsof_output` not found.

- [ ] **Step 3: Implement `parse_lsof_output()`**

Add this function in `src/ssh/discovery.rs`, above `discover_remote_ports()`:

```rust
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
        let name = parts[parts.len() - 2];

        // Handle wildcard "*:port" → "0.0.0.0:port"
        let name = if name.starts_with('*') {
            &name.replacen('*', "0.0.0.0", 1)
        } else {
            name
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
```

Note: The wildcard replacement creates a temporary `String`. Adjust the function to handle ownership:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ssh::discovery::tests::test_parse_lsof -- --nocapture 2>&1`
Expected: All 8 `test_parse_lsof_*` tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ssh/discovery.rs
git commit -m "feat: add lsof output parser for local port discovery"
```

---

### Task 2: `discover_local_ports()` Function

**Files:**
- Modify: `src/ssh/discovery.rs`

- [ ] **Step 1: Write integration test for `discover_local_ports()`**

Add to the test module in `src/ssh/discovery.rs`:

```rust
#[tokio::test]
async fn test_discover_local_ports_finds_something() {
    // The test runner itself (or OS services) will have listening ports
    let ports = discover_local_ports().await;
    assert!(
        !ports.is_empty(),
        "Expected at least one listening port on localhost"
    );
    // Every port should be valid
    for p in &ports {
        assert!(p.port > 0);
        assert!(!p.bind_address.is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib ssh::discovery::tests::test_discover_local_ports 2>&1 | head -20`
Expected: Compilation error — `discover_local_ports` not found.

- [ ] **Step 3: Implement `discover_local_ports()`**

Add this function in `src/ssh/discovery.rs`, below `discover_remote_ports()`. Also add `use tokio::process::Command;` at the top of the file:

Add the import at the top of `src/ssh/discovery.rs`:

```rust
use tokio::process::Command;
```

Add the function below `discover_remote_ports()`:

```rust
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
    let output = Command::new("ss")
        .args(["-tlnp"])
        .output()
        .await;

    if let Ok(out) = output {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.lines().count() > 1 {
                return parse_ss_output(&stdout);
            }
        }
    }

    // Fall back to netstat
    let output = Command::new("netstat")
        .args(["-tlnp"])
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_netstat_output(&stdout)
        }
        _ => Vec::new(),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib ssh::discovery::tests::test_discover_local_ports -- --nocapture 2>&1`
Expected: PASS (at least one listening port found).

- [ ] **Step 5: Run all discovery tests to ensure no regressions**

Run: `cargo test --lib ssh::discovery 2>&1`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/ssh/discovery.rs
git commit -m "feat: add discover_local_ports with platform-adaptive detection"
```

---

### Task 3: `ViewMode` and AppState Changes

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Step 1: Write failing tests for ViewMode state**

Add these tests to the existing `#[cfg(test)] mod tests` block in `src/tui/app.rs`:

```rust
// ---- ViewMode tests ----

#[test]
fn test_new_state_defaults_to_remote_view() {
    let state = AppState::new("host".to_string());
    assert_eq!(state.view_mode, ViewMode::Remote);
    assert!(state.local_ports.is_empty());
    assert_eq!(state.local_selected, 0);
}

#[test]
fn test_toggle_view_mode() {
    let mut state = AppState::new("host".to_string());
    assert_eq!(state.view_mode, ViewMode::Remote);
    state.toggle_view();
    assert_eq!(state.view_mode, ViewMode::Local);
    state.toggle_view();
    assert_eq!(state.view_mode, ViewMode::Remote);
}

#[test]
fn test_local_selected_independent_of_selected() {
    let mut state = AppState::new("host".to_string());
    state.update_ports(vec![make_port(8080, "a"), make_port(3000, "b")]);
    state.update_local_ports(vec![make_port(5000, "c"), make_port(6000, "d"), make_port(7000, "e")]);
    state.selected = 1;
    state.local_selected = 2;
    assert_eq!(state.selected, 1);
    assert_eq!(state.local_selected, 2);
}

#[test]
fn test_update_local_ports() {
    let mut state = AppState::new("host".to_string());
    let ports = vec![make_port(3000, "node"), make_port(8080, "nginx")];
    state.update_local_ports(ports);
    assert_eq!(state.local_ports.len(), 2);
    assert_eq!(state.local_ports[0].port, 3000);
    assert_eq!(state.local_ports[1].port, 8080);
}

#[test]
fn test_update_local_ports_clamps_selection() {
    let mut state = AppState::new("host".to_string());
    state.update_local_ports(vec![make_port(3000, "a"), make_port(5000, "b")]);
    state.local_selected = 1;
    state.update_local_ports(vec![make_port(3000, "a")]);
    assert_eq!(state.local_selected, 0);
}

#[test]
fn test_update_local_ports_does_not_affect_remote_ports() {
    let mut state = AppState::new("host".to_string());
    state.update_ports(vec![make_port(8080, "nginx")]);
    state.set_forward_active(0, 8080);
    state.update_local_ports(vec![make_port(3000, "node")]);
    assert_eq!(state.ports.len(), 1);
    assert_eq!(
        state.ports[0].forward_status,
        ForwardStatus::Active { local_port: 8080 }
    );
}

#[test]
fn test_local_move_up() {
    let mut state = AppState::new("host".to_string());
    state.update_local_ports(vec![make_port(3000, "a"), make_port(5000, "b")]);
    state.local_selected = 1;
    state.local_move_up();
    assert_eq!(state.local_selected, 0);
    state.local_move_up();
    assert_eq!(state.local_selected, 0); // can't go below 0
}

#[test]
fn test_local_move_down() {
    let mut state = AppState::new("host".to_string());
    state.update_local_ports(vec![make_port(3000, "a"), make_port(5000, "b")]);
    state.local_move_down();
    assert_eq!(state.local_selected, 1);
    state.local_move_down();
    assert_eq!(state.local_selected, 1); // can't go past end
}

#[test]
fn test_local_move_on_empty() {
    let mut state = AppState::new("host".to_string());
    state.local_move_up();
    assert_eq!(state.local_selected, 0);
    state.local_move_down();
    assert_eq!(state.local_selected, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::app::tests::test_new_state_defaults_to_remote 2>&1 | head -20`
Expected: Compilation error — `ViewMode` and new fields/methods not found.

- [ ] **Step 3: Implement ViewMode and AppState changes**

In `src/tui/app.rs`, add the `ViewMode` enum after the `InputMode` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Remote,
    Local,
}
```

Add new fields to `AppState`:

```rust
pub struct AppState {
    pub host_alias: String,
    pub connection: ConnectionState,
    pub ports: Vec<PortEntry>,
    pub selected: usize,
    pub input_mode: InputMode,
    pub status_message: Option<String>,
    pub view_mode: ViewMode,
    pub local_ports: Vec<DiscoveredPort>,
    pub local_selected: usize,
}
```

Update `AppState::new()`:

```rust
pub fn new(host_alias: String) -> Self {
    Self {
        host_alias,
        connection: ConnectionState::Connected,
        ports: Vec::new(),
        selected: 0,
        input_mode: InputMode::Normal,
        status_message: None,
        view_mode: ViewMode::Remote,
        local_ports: Vec::new(),
        local_selected: 0,
    }
}
```

Add new methods to the `impl AppState` block:

```rust
pub fn toggle_view(&mut self) {
    self.view_mode = match self.view_mode {
        ViewMode::Remote => ViewMode::Local,
        ViewMode::Local => ViewMode::Remote,
    };
}

pub fn update_local_ports(&mut self, ports: Vec<DiscoveredPort>) {
    self.local_ports = ports;
    if self.local_selected >= self.local_ports.len() && !self.local_ports.is_empty() {
        self.local_selected = self.local_ports.len() - 1;
    }
}

pub fn local_move_up(&mut self) {
    if self.local_selected > 0 {
        self.local_selected -= 1;
    }
}

pub fn local_move_down(&mut self) {
    if self.local_selected + 1 < self.local_ports.len() {
        self.local_selected += 1;
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tui::app 2>&1`
Expected: All tests pass (existing + new).

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat: add ViewMode and local port state to AppState"
```

---

### Task 4: View-Mode-Aware Input Handling

**Files:**
- Modify: `src/tui/input.rs`

- [ ] **Step 1: Write failing tests for Local view input**

Add these tests to the existing `#[cfg(test)] mod tests` block in `src/tui/input.rs`. Add `use crate::tui::app::ViewMode;` to the test module imports:

```rust
// ---- Local view mode tests ----

fn state_with_local_ports() -> AppState {
    let mut state = AppState::new("host".to_string());
    state.update_local_ports(vec![make_port(3000), make_port(5000), make_port(8080)]);
    state.view_mode = ViewMode::Local;
    state
}

#[test]
fn test_tab_switches_to_local() {
    let mut state = state_with_ports();
    assert!(matches!(handle_key(&mut state, key(KeyCode::Tab)), Action::None));
    assert_eq!(state.view_mode, ViewMode::Local);
}

#[test]
fn test_tab_switches_back_to_remote() {
    let mut state = state_with_local_ports();
    assert!(matches!(handle_key(&mut state, key(KeyCode::Tab)), Action::None));
    assert_eq!(state.view_mode, ViewMode::Remote);
}

#[test]
fn test_local_navigate_down() {
    let mut state = state_with_local_ports();
    handle_key(&mut state, key(KeyCode::Down));
    assert_eq!(state.local_selected, 1);
    assert_eq!(state.selected, 0); // remote cursor unchanged
}

#[test]
fn test_local_navigate_up() {
    let mut state = state_with_local_ports();
    state.local_selected = 2;
    handle_key(&mut state, key(KeyCode::Up));
    assert_eq!(state.local_selected, 1);
}

#[test]
fn test_local_navigate_j_k() {
    let mut state = state_with_local_ports();
    handle_key(&mut state, key(KeyCode::Char('j')));
    assert_eq!(state.local_selected, 1);
    handle_key(&mut state, key(KeyCode::Char('k')));
    assert_eq!(state.local_selected, 0);
}

#[test]
fn test_local_enter_is_noop() {
    let mut state = state_with_local_ports();
    assert!(matches!(handle_key(&mut state, key(KeyCode::Enter)), Action::None));
}

#[test]
fn test_local_p_is_noop() {
    let mut state = state_with_local_ports();
    handle_key(&mut state, key(KeyCode::Char('p')));
    assert_eq!(state.input_mode, InputMode::Normal);
}

#[test]
fn test_local_c_is_noop() {
    let mut state = state_with_local_ports();
    assert!(matches!(handle_key(&mut state, key(KeyCode::Char('c'))), Action::None));
}

#[test]
fn test_local_r_refreshes() {
    let mut state = state_with_local_ports();
    assert!(matches!(handle_key(&mut state, key(KeyCode::Char('r'))), Action::Refresh));
}

#[test]
fn test_local_q_quits() {
    let mut state = state_with_local_ports();
    assert!(matches!(handle_key(&mut state, key(KeyCode::Char('q'))), Action::Quit));
}

#[test]
fn test_tab_in_port_input_mode_is_noop() {
    let mut state = state_with_ports();
    state.input_mode = InputMode::PortInput("80".to_string());
    handle_key(&mut state, key(KeyCode::Tab));
    assert_eq!(state.input_mode, InputMode::PortInput("80".to_string()));
    assert_eq!(state.view_mode, ViewMode::Remote);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::input::tests::test_tab_switches 2>&1 | head -20`
Expected: Compilation error or test failures — Tab not handled, `ViewMode` not imported in context.

- [ ] **Step 3: Implement view-mode-aware input handling**

Replace `handle_normal_mode` in `src/tui/input.rs`:

```rust
fn handle_normal_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Tab => {
            state.toggle_view();
            Action::None
        }
        KeyCode::Char('r') => Action::Refresh,
        _ => match state.view_mode {
            ViewMode::Remote => handle_remote_mode(state, key),
            ViewMode::Local => handle_local_mode(state, key),
        },
    }
}

fn handle_remote_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
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

fn handle_local_mode(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            state.local_move_up();
            Action::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.local_move_down();
            Action::None
        }
        _ => Action::None,
    }
}
```

Add the `ViewMode` import at the top of `src/tui/input.rs`:

```rust
use super::app::{AppState, InputMode, ViewMode};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tui::input 2>&1`
Expected: All tests pass (existing + new).

- [ ] **Step 5: Commit**

```bash
git add src/tui/input.rs
git commit -m "feat: add view-mode-aware input handling with Tab toggle"
```

---

### Task 5: View-Mode-Aware UI Rendering

**Files:**
- Modify: `src/tui/ui.rs`

- [ ] **Step 1: Update `render_status_bar` to show view mode label**

In `src/tui/ui.rs`, add `ViewMode` to the import:

```rust
use super::app::{AppState, ConnectionState, ForwardStatus, InputMode, ViewMode};
```

In `render_status_bar`, add the view label after the connection status spans. Replace the existing `let mut spans = vec![...]` block:

```rust
let view_label = match &state.view_mode {
    ViewMode::Remote => "[Remote]",
    ViewMode::Local => "[Local]",
};

let mut spans = vec![
    Span::styled(" portfwd", Style::default().add_modifier(Modifier::BOLD)),
    Span::raw(" — "),
    Span::raw(&state.host_alias),
    Span::raw(" ("),
    Span::styled(conn_label, Style::default().fg(conn_color)),
    Span::raw(") "),
    Span::styled(view_label, Style::default().add_modifier(Modifier::BOLD)),
];
```

- [ ] **Step 2: Update `render_port_table` to handle both views**

Replace the `render_port_table` function:

```rust
fn render_port_table(f: &mut Frame, state: &AppState, area: Rect) {
    match state.view_mode {
        ViewMode::Remote => render_remote_table(f, state, area),
        ViewMode::Local => render_local_table(f, state, area),
    }
}

fn render_remote_table(f: &mut Frame, state: &AppState, area: Rect) {
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
                ForwardStatus::Active { local_port } => (
                    Span::styled("●", Style::default().fg(Color::Green)),
                    format!("localhost:{}", local_port),
                ),
                ForwardStatus::Idle => (
                    Span::styled("○", Style::default().fg(Color::DarkGray)),
                    String::new(),
                ),
                ForwardStatus::Error(msg) => (
                    Span::styled("✗", Style::default().fg(Color::Red)),
                    msg.clone(),
                ),
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
    .block(Block::default().borders(Borders::ALL).title(" Remote Ports "));

    f.render_widget(table, area);
}

fn render_local_table(f: &mut Frame, state: &AppState, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Bind Address"),
        Cell::from("Port"),
        Cell::from("Process"),
        Cell::from("PID"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = state
        .local_ports
        .iter()
        .enumerate()
        .map(|(i, port)| {
            let process = port.process_name.as_deref().unwrap_or("-");
            let pid = port
                .pid
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());

            let style = if i == state.local_selected {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(port.bind_address.clone()),
                Cell::from(port.port.to_string()),
                Cell::from(process.to_string()),
                Cell::from(pid),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(" Local Ports "));

    f.render_widget(table, area);
}
```

- [ ] **Step 3: Update `render_help_bar` for both views**

Replace the `InputMode::Normal` arm in `render_help_bar`:

```rust
InputMode::Normal => match state.view_mode {
    ViewMode::Remote => Line::from(vec![
        Span::styled("[enter]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" toggle  "),
        Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" refresh  "),
        Span::styled("[p]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" change port  "),
        Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" local  "),
        Span::styled("[c]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" reconnect  "),
        Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]),
    ViewMode::Local => Line::from(vec![
        Span::styled("[tab]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" remote  "),
        Span::styled("[r]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" refresh  "),
        Span::styled("[q]", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]),
},
```

- [ ] **Step 4: Verify compilation**

Run: `cargo build 2>&1`
Expected: Compiles without errors.

- [ ] **Step 5: Run all tests**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: add view-mode-aware UI rendering for local/remote views"
```

---

### Task 6: Main Loop Integration

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add concurrent local discovery at startup**

In `src/main.rs`, add the import at the top:

```rust
use ssh::discovery::{discover_remote_ports, discover_local_ports};
```

(Remove the existing `use ssh::discovery::discover_remote_ports;` line.)

Replace the sequential discovery block:

```rust
// Discover ports
let discovered = discover_remote_ports(&session).await?;
```

With concurrent discovery:

```rust
// Discover ports (remote and local concurrently)
let (remote_result, local_ports) = tokio::join!(
    discover_remote_ports(&session),
    discover_local_ports()
);
let discovered = remote_result?;
```

After `state.update_ports(discovered);`, add:

```rust
state.update_local_ports(local_ports);
```

- [ ] **Step 2: Add view-mode-aware refresh in the main loop**

In `src/main.rs`, add `ViewMode` to the imports:

```rust
use tui::app::{AppState, ForwardStatus, ViewMode};
```

(Replace the existing `use tui::app::{AppState, ForwardStatus};` line.)

Replace the `Action::Refresh` arm in `run_loop`:

```rust
Action::Refresh => {
    state.status_message = Some("Scanning...".to_string());
    terminal.draw(|f| render(f, state))?;
    match state.view_mode {
        ViewMode::Remote => {
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
        ViewMode::Local => {
            let ports = discover_local_ports().await;
            state.update_local_ports(ports);
            state.status_message = None;
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1`
Expected: Compiles without errors.

- [ ] **Step 4: Run all tests**

Run: `cargo test 2>&1`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: integrate local discovery into startup and refresh loop"
```

---

### Task 7: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass — no regressions.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No warnings.

- [ ] **Step 3: Verify build in release mode**

Run: `cargo build --release 2>&1`
Expected: Compiles without errors.

- [ ] **Step 4: Commit any clippy fixes (if needed)**

```bash
git add -A
git commit -m "fix: address clippy warnings"
```
