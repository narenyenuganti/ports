//! Requests and daemon messages exchanged over the socket.
//!
//! All types here form the newline-delimited JSON contract. Tagged enums use
//! an internal tag field (`type`, `state`, or `event`) so the Swift `Codable`
//! mirror can decode them with the same discriminator.

use serde::{Deserialize, Serialize};

use crate::protocol::error::ProtocolError;
use crate::protocol::ids::Port;

/// A request sent from the client (Swift app) to the daemon.
///
/// The numeric `id` correlates the request with its `Ack`; the request body is
/// flattened so the JSON object carries both `id` and the `type`-tagged body
/// fields at the top level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Client-chosen correlation id, echoed back in the matching `Ack`.
    pub id: u64,
    /// The action being requested.
    #[serde(flatten)]
    pub body: RequestBody,
}

/// The action carried by a [`Request`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RequestBody {
    /// Set the active host and runtime configuration.
    SetConfig {
        /// SSH config alias to target.
        host_alias: String,
        /// How often the daemon should refresh port state, in seconds.
        refresh_secs: u64,
        /// Whether the daemon should auto-reconnect on connection loss.
        auto_reconnect: bool,
    },
    /// Connect to the configured host.
    Connect,
    /// Disconnect from the host.
    Disconnect,
    /// Force an immediate refresh of port state.
    Refresh,
    /// Start forwarding a remote port to a local port.
    StartForward {
        /// The remote port to forward.
        remote_port: Port,
        /// The desired local port; the daemon may choose one if omitted.
        local_port: Option<Port>,
    },
    /// Stop forwarding a previously started remote port.
    StopForward {
        /// The remote port whose forward should be stopped.
        remote_port: Port,
    },
    /// Send a local file to the host.
    SendFile {
        /// Path to the local file to send.
        local_path: String,
        /// Optional destination path on the host.
        remote_path: Option<String>,
    },
    /// List the host aliases available in the SSH config.
    ListHosts,
    /// Liveness probe.
    Ping,
    /// Ask the daemon to shut down.
    Shutdown,
}

/// A message sent from the daemon to the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMessage {
    /// A full snapshot of current daemon state.
    State(StateSnapshot),
    /// Acknowledgement of a request, optionally carrying an error or a host
    /// list (for `ListHosts`).
    Ack {
        /// The id of the request being acknowledged.
        id: u64,
        /// Present only when the request failed.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ProtocolError>,
        /// Present only for a successful `ListHosts`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hosts: Option<Vec<String>>,
    },
    /// An asynchronous event not tied to a specific request.
    Event(DaemonEvent),
}

/// A complete snapshot of the daemon's connection and port state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// The currently configured host alias, if any.
    pub host: Option<String>,
    /// The current connection status.
    pub status: ConnStatus,
    /// Extra human-readable detail about the status (e.g. an error message).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_detail: Option<String>,
    /// The known ports and their forwarding state.
    pub ports: Vec<PortEntry>,
}

/// The daemon's connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnStatus {
    /// Not connected to any host.
    Disconnected,
    /// A connection attempt is in progress.
    Connecting,
    /// Connected to the host.
    Connected,
    /// Connection is in an error state.
    Error,
}

/// A single port observed on the host and its forwarding state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortEntry {
    /// The remote port number.
    pub remote_port: Port,
    /// The process listening on the port, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    /// The pid of the listening process, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// The forwarding state for this port.
    pub forward: ForwardState,
}

/// The forwarding state of a single port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ForwardState {
    /// Not currently forwarded.
    Idle,
    /// Forwarded to a local port.
    Forwarding {
        /// The local port the remote port is forwarded to.
        local_port: Port,
    },
    /// Forwarding failed.
    Error {
        /// Human-readable description of the failure.
        detail: String,
    },
}

/// An asynchronous event emitted by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DaemonEvent {
    /// The result of a file transfer.
    FileTransfer {
        /// Whether the transfer succeeded.
        ok: bool,
        /// Human-readable detail about the transfer.
        detail: String,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::protocol::ids::Port;

    fn round_trip<T>(value: &T) -> String
    where
        T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let back: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, value, "round trip mismatch for {json}");
        json
    }

    #[test]
    fn request_flattens_id_and_type() {
        let req = Request {
            id: 42,
            body: RequestBody::StartForward {
                remote_port: Port(5432),
                local_port: Some(Port(15432)),
            },
        };
        let json = round_trip(&req);
        assert!(json.contains(r#""id":42"#), "got: {json}");
        assert!(json.contains(r#""type":"start_forward""#), "got: {json}");
    }

    #[test]
    fn request_body_unit_variant_tag() {
        let json = round_trip(&RequestBody::Connect);
        assert_eq!(json, r#"{"type":"connect"}"#);
    }

    #[test]
    fn set_config_tag() {
        let json = round_trip(&RequestBody::SetConfig {
            host_alias: "prod".into(),
            refresh_secs: 5,
            auto_reconnect: true,
        });
        assert!(json.contains(r#""type":"set_config""#), "got: {json}");
    }

    #[test]
    fn daemon_message_state_tag() {
        let msg = DaemonMessage::State(StateSnapshot {
            host: Some("prod".into()),
            status: ConnStatus::Connected,
            status_detail: None,
            ports: vec![],
        });
        let json = round_trip(&msg);
        assert!(json.contains(r#""type":"state""#), "got: {json}");
        assert!(json.contains(r#""status":"connected""#), "got: {json}");
        // status_detail omitted when None.
        assert!(!json.contains("status_detail"), "got: {json}");
    }

    #[test]
    fn ack_without_error_or_hosts_omits_them() {
        let msg = DaemonMessage::Ack {
            id: 7,
            error: None,
            hosts: None,
        };
        let json = round_trip(&msg);
        assert!(json.contains(r#""type":"ack""#), "got: {json}");
        assert!(!json.contains("error"), "got: {json}");
        assert!(!json.contains("hosts"), "got: {json}");
    }

    #[test]
    fn ack_with_error_and_hosts() {
        let msg = DaemonMessage::Ack {
            id: 9,
            error: Some(ProtocolError::NotConnected),
            hosts: Some(vec!["a".into(), "b".into()]),
        };
        let json = round_trip(&msg);
        assert!(json.contains(r#""kind":"not_connected""#), "got: {json}");
        assert!(json.contains(r#""hosts":["a","b"]"#), "got: {json}");
    }

    #[test]
    fn daemon_message_event_tag() {
        let msg = DaemonMessage::Event(DaemonEvent::FileTransfer {
            ok: true,
            detail: "sent".into(),
        });
        let json = round_trip(&msg);
        assert!(json.contains(r#""type":"event""#), "got: {json}");
        assert!(json.contains(r#""event":"file_transfer""#), "got: {json}");
    }

    #[test]
    fn forward_state_variants() {
        assert_eq!(round_trip(&ForwardState::Idle), r#"{"state":"idle"}"#);
        let fwd = round_trip(&ForwardState::Forwarding {
            local_port: Port(9000),
        });
        assert!(fwd.contains(r#""state":"forwarding""#), "got: {fwd}");
        let err = round_trip(&ForwardState::Error {
            detail: "boom".into(),
        });
        assert!(err.contains(r#""state":"error""#), "got: {err}");
    }

    #[test]
    fn conn_status_snake_case() {
        assert_eq!(round_trip(&ConnStatus::Disconnected), r#""disconnected""#);
        assert_eq!(round_trip(&ConnStatus::Connecting), r#""connecting""#);
        assert_eq!(round_trip(&ConnStatus::Connected), r#""connected""#);
        assert_eq!(round_trip(&ConnStatus::Error), r#""error""#);
    }

    #[test]
    fn port_entry_omits_optional_fields() {
        let entry = PortEntry {
            remote_port: Port(8080),
            process: None,
            pid: None,
            forward: ForwardState::Idle,
        };
        let json = round_trip(&entry);
        assert!(!json.contains("process"), "got: {json}");
        assert!(!json.contains("pid"), "got: {json}");
    }
}
