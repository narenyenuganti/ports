#![deny(clippy::unwrap_used, clippy::expect_used)]
//! The headless daemon: owns SSH state and speaks the NDJSON protocol over a
//! Unix domain socket.
//!
//! The daemon is built from four cooperating pieces:
//! - [`engine`]: the [`engine::Engine`] abstraction over the SSH core.
//! - [`actor`]: a single task owning all mutable state, published over a watch.
//! - [`server`]: a `UnixListener` with per-connection NDJSON relays.
//! - [`supervise`]: socket path, single-instance guard, and 0600 permissions.

pub mod actor;
pub mod engine;
pub mod server;
pub mod supervise;

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;

use crate::daemon::engine::SshEngine;
use crate::daemon::supervise::InstanceCheck;

/// Run the daemon, binding to `socket` (or the default path when `None`).
///
/// Performs the single-instance check, binds the socket with `0600`
/// permissions, starts the actor and the accept loop, and runs until either a
/// `Shutdown` request (the actor's run loop ends) or `SIGINT`
/// (`tokio::signal::ctrl_c`) is received. On exit it cancels and awaits the
/// actor and server tasks so all forwards are torn down cleanly, then removes
/// the socket file.
pub async fn run(socket: Option<PathBuf>) -> Result<()> {
    let socket_path = socket.unwrap_or_else(supervise::default_socket_path);

    supervise::ensure_socket_dir(&socket_path)?;

    match supervise::check_single_instance(&socket_path).await? {
        InstanceCheck::AlreadyRunning => {
            log::info!(
                "a live daemon already owns {}; not starting a second instance",
                socket_path.display()
            );
            return Ok(());
        }
        InstanceCheck::SafeToBind => {}
    }

    let cancel = CancellationToken::new();

    // Start the actor over the production SSH engine.
    let engine = Box::new(SshEngine::new());
    let (actor, mut actor_join) = actor::spawn(engine, cancel.clone());

    // Bind the socket and lock it down to owner-only before serving.
    let listener = server::bind(&socket_path)?;
    supervise::restrict_socket_permissions(&socket_path)
        .with_context(|| "failed to restrict socket permissions")?;

    log::info!("daemon listening on {}", socket_path.display());

    let server_cancel = cancel.child_token();
    let server_join = tokio::spawn(server::accept_loop(listener, actor, server_cancel));

    // Run until SIGINT or the actor task ends. The actor ends when it receives
    // a `Shutdown` request or when the token is cancelled; either way its join
    // handle resolves and we proceed to cleanup. A `&mut` borrow keeps the join
    // handle owned so it can still be awaited during cancel-and-await below.
    tokio::select! {
        r = tokio::signal::ctrl_c() => {
            match r {
                Ok(()) => log::info!("received SIGINT; shutting down"),
                Err(e) => log::warn!("failed to listen for ctrl_c: {e}"),
            }
        }
        _ = &mut actor_join => {
            log::info!("actor stopped (shutdown requested)");
        }
    }

    // Cancel-and-await: stop the actor and server, then drain both tasks.
    cancel.cancel();
    let _ = actor_join.await;
    let _ = server_join.await;

    // Remove the socket file on a clean exit so the next launch sees no stale
    // socket. Ignore errors (it may already be gone).
    let _ = std::fs::remove_file(&socket_path);

    Ok(())
}
