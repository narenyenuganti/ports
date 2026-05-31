//! The serializable wire error type.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::ids::Port;

/// Error returned to the client over the wire.
///
/// This is the *only* error type that crosses the socket. Internal errors
/// (`anyhow`) must be converted into one of these variants at the boundary,
/// scrubbing any key material, file contents, or sensitive paths from the
/// `detail` strings before they leave the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProtocolError {
    /// An operation needing an active SSH connection was requested while
    /// disconnected.
    #[error("not connected")]
    NotConnected,

    /// Establishing the SSH connection failed.
    #[error("connect failed: {detail}")]
    ConnectFailed {
        /// Scrubbed, human-readable description of the failure.
        detail: String,
    },

    /// Binding the local listener for a forward failed.
    #[error("bind failed on port {port:?}: {detail}")]
    BindFailed {
        /// The local port that could not be bound.
        port: Port,
        /// Scrubbed, human-readable description of the failure.
        detail: String,
    },

    /// The requested host alias is not present in the SSH config.
    #[error("unknown host: {alias}")]
    UnknownHost {
        /// The alias that could not be resolved.
        alias: String,
    },

    /// Sending a file over the connection failed.
    #[error("send file failed: {detail}")]
    SendFileFailed {
        /// Scrubbed, human-readable description of the failure.
        detail: String,
    },

    /// The request could not be parsed or was otherwise malformed.
    #[error("bad request: {detail}")]
    BadRequest {
        /// Scrubbed, human-readable description of what was wrong.
        detail: String,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::protocol::ids::Port;

    #[test]
    fn bind_failed_serializes_with_snake_case_kind() {
        let err = ProtocolError::BindFailed {
            port: Port(8080),
            detail: "address in use".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains(r#""kind":"bind_failed""#), "got: {json}");
        let back: ProtocolError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, err);
    }

    #[test]
    fn not_connected_round_trips() {
        let err = ProtocolError::NotConnected;
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains(r#""kind":"not_connected""#), "got: {json}");
        let back: ProtocolError = serde_json::from_str(&json).unwrap();
        assert_eq!(back, err);
    }
}
