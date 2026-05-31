#![deny(clippy::unwrap_used, clippy::expect_used)]
//! Daemon supervision: socket path, single-instance guard, and permissions.
//!
//! These helpers decide *where* the socket lives and *whether* it is safe to
//! bind there. The single-instance rule (per the spec) is: if a socket file
//! exists and a live daemon answers a `Ping` over it, refuse to start; if the
//! file exists but nothing answers, treat it as stale and remove it so a fresh
//! bind can succeed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::protocol::message::DaemonMessage;

/// The application-support subdirectory that holds the daemon socket.
const APP_SUPPORT_SUBDIR: &str = "Library/Application Support/com.ports.app";

/// File name of the daemon's Unix socket.
const SOCKET_FILE: &str = "daemon.sock";

/// How long to wait for a `Ping` ack when probing for a live daemon.
const PING_TIMEOUT: Duration = Duration::from_millis(500);

/// The default socket path: `~/Library/Application Support/com.ports.app/daemon.sock`.
///
/// Falls back to the current directory if the home directory cannot be
/// determined, so the daemon never panics on a missing `$HOME`.
pub fn default_socket_path() -> PathBuf {
    let home = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(APP_SUPPORT_SUBDIR).join(SOCKET_FILE)
}

/// Ensure the parent directory of `socket` exists.
pub fn ensure_socket_dir(socket: &Path) -> Result<()> {
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating socket directory {}", parent.display()))?;
    }
    Ok(())
}

/// Restrict `socket` to owner-only access (mode `0600`).
pub fn restrict_socket_permissions(socket: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(socket, perms)
        .with_context(|| format!("setting 0600 permissions on {}", socket.display()))
}

/// Probe whether a live daemon is already listening on `socket`.
///
/// Connects, sends a `Ping`, and waits briefly for any [`DaemonMessage`]. A
/// reply (the daemon emits at least an initial `State` and a `Ping` ack) means
/// a daemon is alive. Any connect/IO/timeout failure is treated as "no live
/// daemon" so a stale socket file does not block startup.
pub async fn is_daemon_alive(socket: &Path) -> bool {
    let attempt = async {
        let stream = UnixStream::connect(socket).await.ok()?;
        let (read_half, mut write_half) = stream.into_split();
        write_half
            .write_all(b"{\"id\":1,\"type\":\"ping\"}\n")
            .await
            .ok()?;
        let mut lines = BufReader::new(read_half).lines();
        // Read lines until we see any well-formed daemon message.
        loop {
            let line = lines.next_line().await.ok()??;
            if serde_json::from_str::<DaemonMessage>(&line).is_ok() {
                return Some(());
            }
        }
    };
    matches!(tokio::time::timeout(PING_TIMEOUT, attempt).await, Ok(Some(())))
}

/// Outcome of the single-instance check.
#[derive(Debug, PartialEq, Eq)]
pub enum InstanceCheck {
    /// No daemon is running; it is safe to bind. A stale socket file (if any)
    /// has been removed.
    SafeToBind,
    /// A live daemon already owns the socket; the caller should not start.
    AlreadyRunning,
}

/// Decide whether starting a new daemon at `socket` is safe.
///
/// If the socket file exists and a live daemon answers a `Ping`, returns
/// [`InstanceCheck::AlreadyRunning`]. Otherwise removes any stale socket file
/// and returns [`InstanceCheck::SafeToBind`].
pub async fn check_single_instance(socket: &Path) -> Result<InstanceCheck> {
    if socket.exists() {
        if is_daemon_alive(socket).await {
            return Ok(InstanceCheck::AlreadyRunning);
        }
        // Stale socket: remove so a fresh bind can succeed.
        std::fs::remove_file(socket)
            .with_context(|| format!("removing stale socket {}", socket.display()))?;
    }
    Ok(InstanceCheck::SafeToBind)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn temp_socket_path() -> PathBuf {
        let unique = format!(
            "ports-sup-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn default_socket_path_ends_with_expected_segments() {
        let path = default_socket_path();
        assert!(path.ends_with("Library/Application Support/com.ports.app/daemon.sock"));
    }

    #[tokio::test]
    async fn missing_socket_is_safe_to_bind() {
        let path = temp_socket_path();
        assert!(!path.exists());
        let check = check_single_instance(&path).await.unwrap();
        assert_eq!(check, InstanceCheck::SafeToBind);
    }

    #[tokio::test]
    async fn stale_socket_file_is_removed_and_safe_to_bind() {
        let path = temp_socket_path();
        // Create a plain file where the socket would be; nothing listens on it.
        std::fs::write(&path, b"stale").unwrap();
        assert!(path.exists());
        let check = check_single_instance(&path).await.unwrap();
        assert_eq!(check, InstanceCheck::SafeToBind);
        // The stale file must have been removed.
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn is_daemon_alive_false_for_nonexistent_socket() {
        let path = temp_socket_path();
        assert!(!is_daemon_alive(&path).await);
    }

    #[tokio::test]
    async fn live_daemon_is_detected_and_refuses_second_instance() {
        use crate::daemon::actor::spawn;
        use crate::daemon::engine::MockEngine;
        use crate::daemon::server::{accept_loop, bind};
        use tokio_util::sync::CancellationToken;

        let path = temp_socket_path();
        let cancel = CancellationToken::new();
        let (actor, _join) = spawn(Box::new(MockEngine::with_ports(vec![])), cancel.clone());
        let listener = bind(&path).unwrap();
        let server = tokio::spawn(accept_loop(listener, actor, cancel.clone()));

        // A live daemon answers Ping.
        assert!(is_daemon_alive(&path).await);
        // ...so a second instance must refuse to start.
        let check = check_single_instance(&path).await.unwrap();
        assert_eq!(check, InstanceCheck::AlreadyRunning);

        cancel.cancel();
        let _ = server.await;
        let _ = std::fs::remove_file(&path);
    }
}
