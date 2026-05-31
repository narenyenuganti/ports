#![deny(clippy::unwrap_used, clippy::expect_used)]
//! The headless daemon: owns SSH state and speaks the NDJSON protocol over a
//! Unix domain socket.
//!
//! The daemon is built from four cooperating pieces:
//! - [`engine`]: the [`engine::Engine`] abstraction over the SSH core.
//! - the actor: a single task owning all mutable state (added in a later task).
//! - the server: a `UnixListener` with per-connection relays (later task).
//! - the supervisor: socket path, single-instance guard, signals (later task).

pub mod engine;

use std::path::PathBuf;

use anyhow::Result;

/// Run the daemon, binding to `socket` (or the default path when `None`).
///
/// This is wired up fully in a later task; for now it is a placeholder so the
/// module compiles while the pieces are built incrementally.
pub async fn run(_socket: Option<PathBuf>) -> Result<()> {
    Ok(())
}
