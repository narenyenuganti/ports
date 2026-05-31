#![deny(clippy::unwrap_used, clippy::expect_used)]
//! The Rust <-> Swift wire protocol.
//!
//! This module defines the pure types that make up the newline-delimited JSON
//! contract spoken between the Rust daemon (`portsd`) and the Swift menu-bar
//! app over a Unix domain socket. It contains **types and serde derives only**:
//! no I/O, no sockets, no runtime dependencies. The serialized shapes are
//! pinned by golden fixtures (see `tests/protocol_fixtures.rs`) so that any
//! drift from the Swift mirror is caught in CI.

pub mod error;
pub mod ids;
pub mod message;
