# File Forwarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Send a local file to a remote machine over the existing SSH connection, with a TUI keybinding (`[f]`) and a CLI subcommand (`ports send-file`).

**Architecture:** Two new `InputMode` variants handle the two-step path entry (local path, then editable remote path). A new `forward/file.rs` module streams file contents over an SSH exec channel (`cat > '/path'`). The CLI subcommand reuses the same transfer function.

**Tech Stack:** Rust, russh (SSH channels), ratatui (TUI), clap (CLI), tokio (async)

---

### Task 1: Add InputMode variants and Action to app.rs

**Files:**
- Modify: `src/tui/app.rs:25-35` (InputMode enum)

- [ ] **Step 1: Add new InputMode variants**

In `src/tui/app.rs`, add two new variants to the `InputMode` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    /// User is typing a port number override
    PortInput(String),
    /// User is selecting a column to sort by
    SortSelect,
    /// User is typing a search query
    Search,
    /// Showing help overlay
    Help,
    /// User is typing a local file path to send
    FilePathInput(String),
    /// User is editing the remote destination path
    RemotePathInput { local: String, remote: String },
}
```

- [ ] **Step 2: Add `file_transfer_status` field to AppState**

In `src/tui/app.rs`, add a field to `AppState`:

```rust
pub struct AppState {
    // ... existing fields ...
    pub file_transfer_status: Option<String>,
}
```

And initialize it in `AppState::new()`:

```rust
file_transfer_status: None,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build 2>&1 | head -20`
Expected: Compile errors in `input.rs` and `ui.rs` because the `match` on `InputMode` is no longer exhaustive. This is expected — we fix those in the next tasks.

- [ ] **Step 4: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat: add FilePathInput and RemotePathInput input modes"
```

---

### Task 2: Add SendFile action and wire up [f] keybinding in input.rs

**Files:**
- Modify: `src/tui/input.rs:1-15` (Action enum)
- Modify: `src/tui/input.rs:17-28` (handle_key)
- Modify: `src/tui/input.rs:49-99` (handle_remote_mode)
- Test: `src/tui/input.rs` (test module)

- [ ] **Step 1: Write failing tests for the [f] keybinding**

Add these tests to the `#[cfg(test)] mod tests` block in `src/tui/input.rs`:

```rust
    // ---- File send mode tests ----

    #[test]
    fn test_f_enters_file_path_input_mode() {
        let mut state = state_with_ports();
        handle_key(&mut state, key(KeyCode::Char('f')));
        assert_eq!(state.input_mode, InputMode::FilePathInput(String::new()));
    }

    #[test]
    fn test_f_in_local_view_is_noop() {
        let mut state = state_with_local_ports();
        handle_key(&mut state, key(KeyCode::Char('f')));
        assert_eq!(state.input_mode, InputMode::Normal);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::input::tests::test_f_enters_file_path_input_mode -- --nocapture 2>&1 | tail -5`
Expected: FAIL (compile error or test failure)

- [ ] **Step 3: Add SendFile variant to Action enum**

In `src/tui/input.rs`, update the `Action` enum:

```rust
pub enum Action {
    None,
    Quit,
    ToggleForward(usize),
    StartForwardWithPort(usize, u16),
    Refresh,
    OpenBrowser(u16),
    ForwardAndOpen(usize),
    SendFile { local: String, remote: String },
}
```

- [ ] **Step 4: Add [f] handler in handle_remote_mode**

In `src/tui/input.rs`, add this arm to `handle_remote_mode` before the `_ => Action::None` catch-all:

```rust
        KeyCode::Char('f') => {
            state.input_mode = InputMode::FilePathInput(String::new());
            Action::None
        }
```

- [ ] **Step 5: Add handle_file_path_input function**

Add this function to `src/tui/input.rs`:

```rust
fn handle_file_path_input(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Enter => {
            if let InputMode::FilePathInput(ref input) = state.input_mode {
                let local_path = input.clone();
                if local_path.is_empty() {
                    state.input_mode = InputMode::Normal;
                    state.status_message = Some("No file path provided".to_string());
                    return Action::None;
                }
                let remote_path = format!("/tmp{}", local_path);
                state.input_mode = InputMode::RemotePathInput {
                    local: local_path,
                    remote: remote_path,
                };
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let InputMode::FilePathInput(ref mut input) = state.input_mode {
                input.pop();
            }
            Action::None
        }
        KeyCode::Char(c) => {
            if let InputMode::FilePathInput(ref mut input) = state.input_mode {
                input.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}
```

- [ ] **Step 6: Add handle_remote_path_input function**

Add this function to `src/tui/input.rs`:

```rust
fn handle_remote_path_input(state: &mut AppState, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        KeyCode::Enter => {
            if let InputMode::RemotePathInput { ref local, ref remote } = state.input_mode {
                let local = local.clone();
                let remote = remote.clone();
                state.input_mode = InputMode::Normal;
                if remote.is_empty() {
                    state.status_message = Some("No remote path provided".to_string());
                    return Action::None;
                }
                return Action::SendFile { local, remote };
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let InputMode::RemotePathInput { ref mut remote, .. } = state.input_mode {
                remote.pop();
            }
            Action::None
        }
        KeyCode::Char(c) => {
            if let InputMode::RemotePathInput { ref mut remote, .. } = state.input_mode {
                remote.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}
```

- [ ] **Step 7: Wire up new modes in handle_key**

Update `handle_key` in `src/tui/input.rs` to dispatch to the new handlers:

```rust
pub fn handle_key(state: &mut AppState, key: KeyEvent) -> Action {
    match &state.input_mode {
        InputMode::Normal => handle_normal_mode(state, key),
        InputMode::PortInput(_) => handle_port_input(state, key),
        InputMode::SortSelect => handle_sort_select(state, key),
        InputMode::Search => handle_search(state, key),
        InputMode::Help => {
            state.input_mode = InputMode::Normal;
            Action::None
        }
        InputMode::FilePathInput(_) => handle_file_path_input(state, key),
        InputMode::RemotePathInput { .. } => handle_remote_path_input(state, key),
    }
}
```

- [ ] **Step 8: Run the two tests from Step 1**

Run: `cargo test --lib tui::input::tests::test_f_ -- --nocapture 2>&1 | tail -10`
Expected: Both PASS

- [ ] **Step 9: Write remaining unit tests for file path input mode**

Add to the test module in `src/tui/input.rs`:

```rust
    #[test]
    fn test_file_path_input_typing() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput(String::new());
        handle_key(&mut state, key(KeyCode::Char('/')));
        handle_key(&mut state, key(KeyCode::Char('t')));
        handle_key(&mut state, key(KeyCode::Char('m')));
        handle_key(&mut state, key(KeyCode::Char('p')));
        assert_eq!(state.input_mode, InputMode::FilePathInput("/tmp".to_string()));
    }

    #[test]
    fn test_file_path_input_backspace() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("/tmp".to_string());
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(state.input_mode, InputMode::FilePathInput("/tm".to_string()));
    }

    #[test]
    fn test_file_path_input_esc_cancels() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("/tmp/foo".to_string());
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_file_path_input_enter_transitions_to_remote_path() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput("/home/user/file.txt".to_string());
        let action = handle_key(&mut state, key(KeyCode::Enter));
        assert!(matches!(action, Action::None));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "/home/user/file.txt".to_string(),
                remote: "/tmp/home/user/file.txt".to_string(),
            }
        );
    }

    #[test]
    fn test_file_path_input_enter_empty_shows_error() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::FilePathInput(String::new());
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.status_message.as_deref(), Some("No file path provided"));
    }

    #[test]
    fn test_remote_path_input_enter_produces_send_file() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp/home/user/file.txt".to_string(),
        };
        let action = handle_key(&mut state, key(KeyCode::Enter));
        match action {
            Action::SendFile { local, remote } => {
                assert_eq!(local, "/home/user/file.txt");
                assert_eq!(remote, "/tmp/home/user/file.txt");
            }
            _ => panic!("Expected SendFile action"),
        }
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_remote_path_input_editing() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp".to_string(),
        };
        handle_key(&mut state, key(KeyCode::Char('/')));
        handle_key(&mut state, key(KeyCode::Char('f')));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "/home/user/file.txt".to_string(),
                remote: "/tmp/f".to_string(),
            }
        );
    }

    #[test]
    fn test_remote_path_input_backspace() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp".to_string(),
        };
        handle_key(&mut state, key(KeyCode::Backspace));
        assert_eq!(
            state.input_mode,
            InputMode::RemotePathInput {
                local: "/home/user/file.txt".to_string(),
                remote: "/tm".to_string(),
            }
        );
    }

    #[test]
    fn test_remote_path_input_esc_cancels() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: "/tmp/home/user/file.txt".to_string(),
        };
        handle_key(&mut state, key(KeyCode::Esc));
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_remote_path_input_enter_empty_shows_error() {
        let mut state = state_with_ports();
        state.input_mode = InputMode::RemotePathInput {
            local: "/home/user/file.txt".to_string(),
            remote: String::new(),
        };
        handle_key(&mut state, key(KeyCode::Enter));
        assert_eq!(state.input_mode, InputMode::Normal);
        assert_eq!(state.status_message.as_deref(), Some("No remote path provided"));
    }
```

- [ ] **Step 10: Run all tests**

Run: `cargo test --lib tui::input 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 11: Commit**

```bash
git add src/tui/input.rs
git commit -m "feat: add [f] keybinding and file path input handlers"
```

---

### Task 3: Update UI rendering for file input modes

**Files:**
- Modify: `src/tui/ui.rs:229-336` (render_help_bar)
- Modify: `src/tui/ui.rs:338-430` (render_help_overlay)

- [ ] **Step 1: Add help bar rendering for FilePathInput mode**

In `src/tui/ui.rs`, in the `render_help_bar` function, add this arm to the `match &state.input_mode` block, after the `InputMode::PortInput` arm:

```rust
        InputMode::FilePathInput(input) => Line::from(vec![
            Span::raw(" Local file path: "),
            Span::styled(input, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::raw("  [enter] confirm  [esc] cancel"),
        ]),
        InputMode::RemotePathInput { remote, .. } => Line::from(vec![
            Span::raw(" Remote path: "),
            Span::styled(remote, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::raw("  [enter] send  [esc] cancel"),
        ]),
```

- [ ] **Step 2: Add [f] to the remote normal mode help bar**

In `src/tui/ui.rs`, in the `render_help_bar` function, in the `ViewMode::Remote` arm under `InputMode::Normal`, add these spans after the `[p] change port` entry:

```rust
                Span::styled("[f]", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" send file  "),
```

- [ ] **Step 3: Add [f] to the help overlay**

In `src/tui/ui.rs`, in the `render_help_overlay` function, in the `ViewMode::Remote` arm, add this entry after the `p` line:

```rust
            Line::from(vec![
                Span::styled("  f      ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw("Send a local file to the remote machine"),
            ]),
```

- [ ] **Step 4: Add file transfer status display to status bar**

In `src/tui/ui.rs`, in the `render_status_bar` function, add after the `status_message` block:

```rust
    if let Some(ref ft_status) = state.file_transfer_status {
        spans.push(Span::styled(
            format!("  {}", ft_status),
            Style::default().fg(Color::Cyan),
        ));
    }
```

- [ ] **Step 5: Verify it compiles and tests pass**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: add file send UI rendering in help bar and overlay"
```

---

### Task 4: Implement file transfer over SSH

**Files:**
- Create: `src/forward/file.rs`
- Modify: `src/forward/mod.rs`

- [ ] **Step 1: Create `src/forward/file.rs` with the transfer function**

```rust
use anyhow::{Context, Result};
use russh::ChannelMsg;
use std::path::Path;

use crate::ssh::connection::SshSession;

/// Send a local file to the remote machine over the SSH connection.
///
/// Opens an exec channel running `cat > '<remote_path>'`, writes the file
/// contents to stdin, then checks the exit status.
pub async fn send_file(
    session: &SshSession,
    local_path: &str,
    remote_path: &str,
) -> Result<()> {
    // Read local file
    let contents = tokio::fs::read(local_path)
        .await
        .with_context(|| format!("Failed to read local file '{}'", local_path))?;

    // Ensure remote parent directory exists, then write file
    let escaped_remote = remote_path.replace('\'', "'\\''");
    let escaped_dir = Path::new(remote_path)
        .parent()
        .unwrap_or(Path::new("/"))
        .to_string_lossy()
        .replace('\'', "'\\''");
    let command = format!("mkdir -p '{}' && cat > '{}'", escaped_dir, escaped_remote);

    let mut channel = session
        .handle
        .channel_open_session()
        .await
        .context("Failed to open SSH session channel for file transfer")?;

    channel
        .exec(true, command.as_bytes())
        .await
        .context("Failed to execute remote cat command")?;

    // Write file contents to channel stdin
    channel
        .data(&contents[..])
        .await
        .context("Failed to write file data to SSH channel")?;

    // Signal end of input
    channel
        .eof()
        .await
        .context("Failed to send EOF on SSH channel")?;

    // Wait for exit status
    let mut exit_status = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::ExitStatus { exit_status: code } => {
                exit_status = Some(code);
            }
            _ => {}
        }
    }

    match exit_status {
        Some(0) => Ok(()),
        Some(code) => anyhow::bail!("Remote command exited with status {}", code),
        None => Ok(()), // No exit status received, assume success
    }
}
```

- [ ] **Step 2: Update `src/forward/mod.rs` to export file module**

```rust
pub mod file;
pub mod tunnel;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Commit**

```bash
git add src/forward/file.rs src/forward/mod.rs
git commit -m "feat: implement file transfer over SSH exec channel"
```

---

### Task 5: Wire up SendFile action in the main event loop

**Files:**
- Modify: `src/main.rs:98-186` (action match block)

- [ ] **Step 1: Add use for send_file and Path**

At the top of `src/main.rs`, add:

```rust
use forward::file::send_file;
use std::path::Path;
```

- [ ] **Step 2: Add SendFile handler in the action match**

In `src/main.rs`, in the `run_loop` function's action match block, add this arm before `Action::None => {}`:

```rust
                    Action::SendFile { local, remote } => {
                        // Validate local file exists
                        if !Path::new(&local).exists() {
                            state.status_message = Some(format!("File not found: {}", local));
                        } else {
                            let filename = Path::new(&local)
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| local.clone());
                            state.file_transfer_status = Some(format!("Sending {}...", filename));
                            terminal.draw(|f| render(f, state))?;

                            match send_file(&session, &local, &remote).await {
                                Ok(()) => {
                                    state.file_transfer_status = None;
                                    state.status_message =
                                        Some(format!("Sent {} -> {}", filename, remote));
                                }
                                Err(e) => {
                                    state.file_transfer_status = None;
                                    state.status_message =
                                        Some(format!("Send failed: {}", e));
                                }
                            }
                        }
                    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up SendFile action in main event loop"
```

---

### Task 6: Add CLI subcommand `send-file`

**Files:**
- Modify: `src/main.rs:25-30` (Cli struct)
- Modify: `src/main.rs:33-79` (main function)

- [ ] **Step 1: Convert CLI to subcommand structure**

Replace the `Cli` struct and `main` function in `src/main.rs`. The `Cli` struct becomes:

```rust
#[derive(Parser)]
#[command(name = "ports", about = "Lightweight SSH port forwarding TUI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// SSH config host alias (for TUI mode)
    host: Option<String>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Send a local file to a remote machine
    SendFile {
        /// SSH config host alias
        host: String,
        /// Local file path
        local_path: String,
        /// Remote destination path (defaults to /tmp/<local_path>)
        remote_path: Option<String>,
    },
}
```

- [ ] **Step 2: Update main() to dispatch subcommands**

Update the `main` function to handle both the TUI and send-file paths:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::SendFile {
            host,
            local_path,
            remote_path,
        }) => {
            run_send_file(&host, &local_path, remote_path.as_deref()).await
        }
        None => {
            let host = cli
                .host
                .context("Host argument required for TUI mode")?;
            run_tui(&host).await
        }
    }
}

async fn run_send_file(host: &str, local_path: &str, remote_path: Option<&str>) -> Result<()> {
    // Validate local file
    if !Path::new(local_path).exists() {
        anyhow::bail!("File not found: {}", local_path);
    }

    let remote_path = match remote_path {
        Some(p) => p.to_string(),
        None => format!("/tmp{}", local_path),
    };

    let host_config = load_host_config(host)
        .with_context(|| format!("Failed to load SSH config for host '{}'", host))?;

    eprintln!(
        "Connecting to {}@{}:{}...",
        host_config.user, host_config.hostname, host_config.port
    );

    let session = SshSession::connect(&host_config).await?;

    let filename = Path::new(local_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| local_path.to_string());

    eprintln!("Sending {}...", filename);
    send_file(&session, local_path, &remote_path).await?;
    eprintln!("Sent {} -> {}", filename, remote_path);

    Ok(())
}
```

- [ ] **Step 3: Extract TUI logic into run_tui function**

Move the existing TUI setup code from `main()` into a new `run_tui` function:

```rust
async fn run_tui(host: &str) -> Result<()> {
    let host_config = load_host_config(host)
        .with_context(|| format!("Failed to load SSH config for host '{}'", host))?;

    eprintln!(
        "Connecting to {}@{}:{}...",
        host_config.user, host_config.hostname, host_config.port
    );

    let session = SshSession::connect(&host_config).await?;
    let session = Arc::new(session);

    let (remote_result, local_ports) = tokio::join!(
        discover_remote_ports(&session),
        discover_local_ports()
    );
    let discovered = remote_result?;

    let mut state = AppState::new(host.to_string());
    state.update_ports(discovered);
    state.update_local_ports(local_ports);

    let mut fwd_manager = ForwardManager::new(session.clone());

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, &mut state, &mut fwd_manager, session, &host_config).await;

    fwd_manager.stop_all();
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo build 2>&1 | tail -10`
Expected: Compiles successfully

- [ ] **Step 5: Verify CLI help works**

Run: `cargo run -- --help 2>&1`
Expected: Shows help with `send-file` subcommand listed

Run: `cargo run -- send-file --help 2>&1`
Expected: Shows help for send-file with host, local_path, remote_path args

- [ ] **Step 6: Verify TUI still works with positional host**

Run: `cargo run -- --help 2>&1`
Expected: Still shows the host argument for TUI mode

- [ ] **Step 7: Run all tests**

Run: `cargo test 2>&1 | tail -5`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add src/main.rs
git commit -m "feat: add send-file CLI subcommand"
```

---

### Task 7: Final integration verification

**Files:** None (verification only)

- [ ] **Step 1: Run the full test suite**

Run: `cargo test 2>&1`
Expected: All tests PASS, no warnings

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Expected: No errors. Fix any warnings.

- [ ] **Step 3: Verify build in release mode**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully
