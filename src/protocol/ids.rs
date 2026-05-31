//! Newtype identifiers used across the protocol.

use serde::{Deserialize, Serialize};

/// Stable identifier for a single port-forward managed by the daemon.
///
/// Serializes transparently as a bare JSON number so the wire stays compact
/// and the Swift mirror can model it as a plain integer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ForwardId(pub u64);

/// A TCP port number (local or remote) used in forwarding requests and state.
///
/// Serializes transparently as a bare JSON number.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Port(pub u16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_id_serializes_transparently() {
        let id = ForwardId(7);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "7");
        let back: ForwardId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn port_round_trips() {
        let port = Port(8080);
        let json = serde_json::to_string(&port).unwrap();
        assert_eq!(json, "8080");
        let back: Port = serde_json::from_str(&json).unwrap();
        assert_eq!(back, port);
    }
}
