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

use forward::file::send_file;
use forward::tunnel::ForwardManager;
use ssh::config::{load_host_config, HostConfig};
use ssh::connection::SshSession;
use ssh::discovery::{discover_local_ports, discover_remote_ports};
use std::path::Path;
use tui::app::{AppState, ForwardStatus, ViewMode};
use tui::input::{handle_key, Action};
use tui::ui::render;

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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::SendFile {
            host,
            local_path,
            remote_path,
        }) => run_send_file(&host, &local_path, remote_path.as_deref()).await,
        None => {
            let host = cli.host.context("Host argument required for TUI mode")?;
            run_tui(&host).await
        }
    }
}

async fn run_tui(host: &str) -> Result<()> {
    let host_config = load_host_config(host)
        .with_context(|| format!("Failed to load SSH config for host '{}'", host))?;

    eprintln!(
        "Connecting to {}@{}:{}...",
        host_config.user, host_config.hostname, host_config.port
    );

    let session = SshSession::connect(&host_config).await?;
    let session = Arc::new(session);

    let (remote_result, local_ports) =
        tokio::join!(discover_remote_ports(&session), discover_local_ports());
    let discovered = remote_result?;

    let mut state = AppState::new(host.to_string());
    state.update_ports(discovered);
    state.update_local_ports(local_ports);

    let mut fwd_manager = ForwardManager::new(session.clone());

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(
        &mut terminal,
        &mut state,
        &mut fwd_manager,
        session,
        &host_config,
    )
    .await;

    fwd_manager.stop_all();
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_send_file(host: &str, local_path: &str, remote_path: Option<&str>) -> Result<()> {
    // Validate local file
    if !Path::new(local_path).exists() {
        anyhow::bail!("File not found: {}", local_path);
    }

    let remote_path = match remote_path {
        Some(p) => p.to_string(),
        None => {
            if local_path.starts_with('/') {
                format!("/tmp{}", local_path)
            } else {
                format!("/tmp/{}", local_path)
            }
        }
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

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut AppState,
    fwd_manager: &mut ForwardManager,
    mut session: Arc<SshSession>,
    host_config: &HostConfig,
) -> Result<()> {
    loop {
        terminal.draw(|f| render(f, state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
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
                        match state.view_mode {
                            ViewMode::Remote => {
                                match discover_remote_ports(&session).await {
                                    Ok(ports) => {
                                        state.update_ports(ports);
                                        state.status_message = None;
                                    }
                                    Err(_) => {
                                        // Discovery failed — try reconnecting
                                        state.status_message = Some("Reconnecting...".to_string());
                                        terminal.draw(|f| render(f, state))?;
                                        match SshSession::connect(host_config).await {
                                            Ok(new_session) => {
                                                session = Arc::new(new_session);
                                                fwd_manager.stop_all();
                                                for i in 0..state.ports.len() {
                                                    state.set_forward_idle(i);
                                                }
                                                fwd_manager.update_session(session.clone());
                                                match discover_remote_ports(&session).await {
                                                    Ok(ports) => {
                                                        state.update_ports(ports);
                                                        state.status_message =
                                                            Some("Reconnected".to_string());
                                                    }
                                                    Err(e) => {
                                                        state.status_message = Some(format!(
                                                            "Scan failed after reconnect: {}",
                                                            e
                                                        ));
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                state.status_message =
                                                    Some(format!("Reconnect failed: {}", e));
                                            }
                                        }
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
                    Action::OpenBrowser(port) => {
                        let url = format!("http://localhost:{}", port);
                        match open::that(&url) {
                            Ok(_) => {
                                state.status_message = Some(format!("Opened {}", url));
                            }
                            Err(e) => {
                                state.status_message =
                                    Some(format!("Failed to open browser: {}", e));
                            }
                        }
                    }
                    Action::ForwardAndOpen(idx) => {
                        toggle_forward(state, fwd_manager, idx).await;
                        if let Some(entry) = state.ports.get(idx) {
                            if let ForwardStatus::Active { local_port } = &entry.forward_status {
                                let url = format!("http://localhost:{}", local_port);
                                match open::that(&url) {
                                    Ok(_) => {
                                        state.status_message = Some(format!("Opened {}", url));
                                    }
                                    Err(e) => {
                                        state.status_message =
                                            Some(format!("Failed to open browser: {}", e));
                                    }
                                }
                            }
                        }
                    }
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
                                    state.status_message = Some(format!("Send failed: {}", e));
                                }
                            }
                        }
                    }
                    Action::None => {}
                }
            }
        }
    }

    Ok(())
}

async fn toggle_forward(state: &mut AppState, fwd_manager: &mut ForwardManager, idx: usize) {
    let entry = match state.ports.get(idx) {
        Some(e) => e.clone(),
        None => return,
    };

    match &entry.forward_status {
        ForwardStatus::Active { .. } => {
            fwd_manager.stop_forward(entry.discovered.port);
            state.set_forward_idle(idx);
            state.status_message = Some(format!(
                "Stopped forward for port {}",
                entry.discovered.port
            ));
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
                        "Forwarding localhost:{} -> remote:{}",
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
                "Forwarding localhost:{} -> remote:{}",
                actual_port, entry.discovered.port
            ));
        }
        Err(e) => {
            state.set_forward_error(idx, format!("{}", e));
            state.status_message = Some(format!("Failed to forward port {}: {}", local_port, e));
        }
    }
}
