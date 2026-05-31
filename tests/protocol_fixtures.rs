//! Golden-fixture drift tests for the wire protocol.
//!
//! Each `check(name, value)` pretty-serializes `value` to JSON and compares it
//! against the committed fixture `tests/protocol_fixtures/<name>.json`. The
//! committed fixtures are the canonical, Swift-mirrored wire shapes; any change
//! to the Rust serialization shows up here as a failing test (drift).
//!
//! To regenerate the fixtures after an intentional protocol change, run:
//!
//! ```sh
//! REGEN_FIXTURES=1 cargo test --test protocol_fixtures
//! ```
//!
//! then review the JSON diff and update the Swift mirror to match.

use std::fs;
use std::path::PathBuf;

use ports::protocol::error::ProtocolError;
use ports::protocol::ids::{ForwardId, Port};
use ports::protocol::message::{
    ConnStatus, DaemonEvent, DaemonMessage, ForwardState, PortEntry, Request, RequestBody,
    StateSnapshot,
};
use serde::Serialize;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("protocol_fixtures")
        .join(format!("{name}.json"))
}

/// Compare `value`'s pretty JSON against the committed fixture, or regenerate
/// the fixture when `REGEN_FIXTURES` is set in the environment.
fn check<T: Serialize>(name: &str, value: &T) {
    let actual = serde_json::to_string_pretty(value).expect("serialize fixture value");
    let path = fixture_path(name);

    if std::env::var_os("REGEN_FIXTURES").is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixtures dir");
        }
        fs::write(&path, format!("{actual}\n")).expect("write fixture");
        return;
    }

    let expected = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing fixture {}: {e}. Run REGEN_FIXTURES=1 cargo test --test protocol_fixtures",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected.trim_end_matches('\n'),
        "fixture drift for {name}; run REGEN_FIXTURES=1 to update and review"
    );
}

#[test]
fn fixtures_match() {
    // Newtype IDs.
    check("forward_id", &ForwardId(7));
    check("port", &Port(8080));

    // ProtocolError variants.
    check("error_not_connected", &ProtocolError::NotConnected);
    check(
        "error_connect_failed",
        &ProtocolError::ConnectFailed {
            detail: "handshake timed out".into(),
        },
    );
    check(
        "error_bind_failed",
        &ProtocolError::BindFailed {
            port: Port(8080),
            detail: "address in use".into(),
        },
    );
    check(
        "error_unknown_host",
        &ProtocolError::UnknownHost {
            alias: "prod".into(),
        },
    );
    check(
        "error_send_file_failed",
        &ProtocolError::SendFileFailed {
            detail: "permission denied".into(),
        },
    );
    check(
        "error_bad_request",
        &ProtocolError::BadRequest {
            detail: "unknown type".into(),
        },
    );

    // Request wrapper (id + flattened body).
    check(
        "request",
        &Request {
            id: 42,
            body: RequestBody::Ping,
        },
    );

    // RequestBody variants (exhaustive).
    check(
        "request_body_set_config",
        &RequestBody::SetConfig {
            host_alias: "prod".into(),
            refresh_secs: 5,
            auto_reconnect: true,
        },
    );
    check("request_body_connect", &RequestBody::Connect);
    check("request_body_disconnect", &RequestBody::Disconnect);
    check("request_body_refresh", &RequestBody::Refresh);
    check(
        "request_body_start_forward",
        &RequestBody::StartForward {
            remote_port: Port(5432),
            local_port: Some(Port(15432)),
        },
    );
    check(
        "request_body_stop_forward",
        &RequestBody::StopForward {
            remote_port: Port(5432),
        },
    );
    check(
        "request_body_send_file",
        &RequestBody::SendFile {
            local_path: "/tmp/data.bin".into(),
            remote_path: Some("/srv/data.bin".into()),
        },
    );
    check("request_body_list_hosts", &RequestBody::ListHosts);
    check("request_body_ping", &RequestBody::Ping);
    check("request_body_shutdown", &RequestBody::Shutdown);

    // DaemonMessage variants.
    check(
        "daemon_message_state",
        &DaemonMessage::State(StateSnapshot {
            host: Some("prod".into()),
            status: ConnStatus::Connected,
            status_detail: None,
            ports: vec![PortEntry {
                remote_port: Port(5432),
                process: Some("postgres".into()),
                pid: Some(1234),
                forward: ForwardState::Forwarding {
                    local_port: Port(15432),
                },
            }],
        }),
    );
    check(
        "daemon_message_ack_ok",
        &DaemonMessage::Ack {
            id: 7,
            error: None,
            hosts: None,
        },
    );
    check(
        "daemon_message_ack_error",
        &DaemonMessage::Ack {
            id: 8,
            error: Some(ProtocolError::NotConnected),
            hosts: None,
        },
    );
    check(
        "daemon_message_ack_hosts",
        &DaemonMessage::Ack {
            id: 9,
            error: None,
            hosts: Some(vec!["prod".into(), "staging".into()]),
        },
    );
    check(
        "daemon_message_event",
        &DaemonMessage::Event(DaemonEvent::FileTransfer {
            ok: true,
            detail: "uploaded 3 files".into(),
        }),
    );

    // StateSnapshot standalone (with detail present).
    check(
        "state_snapshot",
        &StateSnapshot {
            host: None,
            status: ConnStatus::Error,
            status_detail: Some("connection refused".into()),
            ports: vec![],
        },
    );

    // PortEntry standalone (optional fields omitted).
    check(
        "port_entry",
        &PortEntry {
            remote_port: Port(8080),
            process: None,
            pid: None,
            forward: ForwardState::Idle,
        },
    );

    // ConnStatus variants.
    check("conn_status_disconnected", &ConnStatus::Disconnected);
    check("conn_status_connecting", &ConnStatus::Connecting);
    check("conn_status_connected", &ConnStatus::Connected);
    check("conn_status_error", &ConnStatus::Error);

    // ForwardState variants.
    check("forward_state_idle", &ForwardState::Idle);
    check(
        "forward_state_forwarding",
        &ForwardState::Forwarding {
            local_port: Port(9000),
        },
    );
    check(
        "forward_state_error",
        &ForwardState::Error {
            detail: "remote closed".into(),
        },
    );

    // DaemonEvent variant.
    check(
        "daemon_event_file_transfer",
        &DaemonEvent::FileTransfer {
            ok: false,
            detail: "transfer aborted".into(),
        },
    );
}
