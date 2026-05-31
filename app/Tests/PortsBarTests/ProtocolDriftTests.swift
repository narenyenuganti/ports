import Foundation
import Testing
@testable import PortsBarCore

/// Resolve the repo root from this source file's path.
/// #filePath is .../app/Tests/PortsBarTests/ProtocolDriftTests.swift
/// Drop the last 4 components (file, PortsBarTests, Tests, app) to reach root.
private func repoRoot(file: String = #filePath) -> URL {
    var url = URL(fileURLWithPath: file)
    for _ in 0..<4 { url.deleteLastPathComponent() }
    return url
}

private func fixturesDir() -> URL {
    repoRoot().appendingPathComponent("tests/protocol_fixtures", isDirectory: true)
}

private func fixtureData(_ name: String) throws -> Data {
    try Data(contentsOf: fixturesDir().appendingPathComponent(name))
}

private let decoder = JSONDecoder()
private let encoder = JSONEncoder()

/// Categorize a fixture filename to the Swift type it decodes into, then
/// decode + round-trip (re-encode, re-decode, assert equal).
private func decodeAndRoundTrip(_ name: String) throws {
    let data = try fixtureData(name)
    func rt<T: Codable & Equatable>(_ type: T.Type) throws {
        let v = try decoder.decode(T.self, from: data)
        let re = try encoder.encode(v)
        let v2 = try decoder.decode(T.self, from: re)
        #expect(v == v2)
    }

    switch name {
    case let n where n.hasPrefix("conn_status_"):
        try rt(ConnStatus.self)
    case let n where n.hasPrefix("error_"):
        try rt(ProtocolError.self)
    case let n where n.hasPrefix("daemon_message_"):
        try rt(DaemonMessage.self)
    case "daemon_event_file_transfer.json":
        try rt(DaemonEvent.self)
    case let n where n.hasPrefix("request_body_"):
        try rt(RequestBody.self)
    case "request.json":
        try rt(Request.self)
    case "forward_id.json":
        try rt(ForwardId.self)
    case "port.json":
        try rt(Port.self)
    case "port_entry.json":
        try rt(PortEntry.self)
    case let n where n.hasPrefix("forward_state_"):
        try rt(ForwardState.self)
    case "state_snapshot.json":
        try rt(PortsState.self)
    default:
        Issue.record("uncategorized fixture: \(name)")
    }
}

@Suite("Protocol drift")
struct ProtocolDriftTests {
    static let allFixtures = [
        "conn_status_connected.json",
        "conn_status_connecting.json",
        "conn_status_disconnected.json",
        "conn_status_error.json",
        "daemon_event_file_transfer.json",
        "daemon_message_ack_error.json",
        "daemon_message_ack_hosts.json",
        "daemon_message_ack_ok.json",
        "daemon_message_event.json",
        "daemon_message_state.json",
        "error_bad_request.json",
        "error_bind_failed.json",
        "error_connect_failed.json",
        "error_not_connected.json",
        "error_send_file_failed.json",
        "error_unknown_host.json",
        "forward_id.json",
        "forward_state_error.json",
        "forward_state_forwarding.json",
        "forward_state_idle.json",
        "port.json",
        "port_entry.json",
        "request.json",
        "request_body_connect.json",
        "request_body_disconnect.json",
        "request_body_list_hosts.json",
        "request_body_ping.json",
        "request_body_refresh.json",
        "request_body_send_file.json",
        "request_body_set_config.json",
        "request_body_shutdown.json",
        "request_body_start_forward.json",
        "request_body_stop_forward.json",
        "state_snapshot.json",
    ]

    @Test("repo root resolves and fixtures dir has all 34 files")
    func fixturesPresent() throws {
        let contents = try FileManager.default.contentsOfDirectory(
            at: fixturesDir(), includingPropertiesForKeys: nil
        )
        let jsonFiles = contents.filter { $0.pathExtension == "json" }
        #expect(jsonFiles.count == 34)
        #expect(Self.allFixtures.count == 34)
    }

    @Test("every fixture decodes and round-trips", arguments: allFixtures)
    func eachFixtureDecodes(_ name: String) throws {
        try decodeAndRoundTrip(name)
    }

    // MARK: - Targeted contract assertions

    @Test("state fixture: host and first forward")
    func stateFixture() throws {
        let data = try fixtureData("daemon_message_state.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .state(let snapshot) = msg else {
            Issue.record("expected .state")
            return
        }
        #expect(snapshot.host == "prod")
        #expect(snapshot.status == .connected)
        #expect(snapshot.ports.count == 1)
        #expect(snapshot.ports[0].forward == .forwarding(localPort: Port(15432)))
        #expect(snapshot.ports[0].process == "postgres")
        #expect(snapshot.ports[0].pid == 1234)
    }

    @Test("start_forward request body decodes local_port Some")
    func startForwardSome() throws {
        let data = try fixtureData("request_body_start_forward.json")
        let body = try decoder.decode(RequestBody.self, from: data)
        guard case .startForward(let remotePort, let localPort) = body else {
            Issue.record("expected .startForward")
            return
        }
        #expect(remotePort == Port(5432))
        #expect(localPort == Port(15432))
    }

    @Test("start_forward with explicit null local_port decodes to nil")
    func startForwardExplicitNull() throws {
        let json = #"{"id":1,"type":"start_forward","remote_port":3000,"local_port":null}"#
        let body = try decoder.decode(RequestBody.self, from: Data(json.utf8))
        guard case .startForward(let remotePort, let localPort) = body else {
            Issue.record("expected .startForward")
            return
        }
        #expect(remotePort == Port(3000))
        #expect(localPort == nil)
    }

    @Test("start_forward with missing local_port key decodes to nil")
    func startForwardMissingKey() throws {
        let json = #"{"type":"start_forward","remote_port":3000}"#
        let body = try decoder.decode(RequestBody.self, from: Data(json.utf8))
        guard case .startForward(_, let localPort) = body else {
            Issue.record("expected .startForward")
            return
        }
        #expect(localPort == nil)
    }

    @Test("ack_error fixture decodes to .ack with not_connected error")
    func ackErrorFixture() throws {
        let data = try fixtureData("daemon_message_ack_error.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 8)
        #expect(hosts == nil)
        #expect(error == .notConnected)
    }

    @Test("bind_failed error fixture decodes port + detail")
    func bindFailedFixture() throws {
        let data = try fixtureData("error_bind_failed.json")
        let err = try decoder.decode(ProtocolError.self, from: data)
        #expect(err == .bindFailed(port: Port(8080), detail: "address in use"))
    }

    @Test("ack_hosts fixture decodes host list")
    func ackHostsFixture() throws {
        let data = try fixtureData("daemon_message_ack_hosts.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 9)
        #expect(error == nil)
        #expect(hosts == ["prod", "staging"])
    }

    @Test("ack_ok fixture: both optionals nil")
    func ackOkFixture() throws {
        let data = try fixtureData("daemon_message_ack_ok.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 7)
        #expect(error == nil)
        #expect(hosts == nil)
    }

    @Test("file_transfer event (via daemon_message_event) decodes")
    func fileTransferEvent() throws {
        let data = try fixtureData("daemon_message_event.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .event(.fileTransfer(let ok, let detail)) = msg else {
            Issue.record("expected .event(.fileTransfer)")
            return
        }
        #expect(ok == true)
        #expect(detail == "uploaded 3 files")
    }

    @Test("state_snapshot fixture: nil host, error status, status_detail present")
    func stateSnapshotFixture() throws {
        let data = try fixtureData("state_snapshot.json")
        let snapshot = try decoder.decode(PortsState.self, from: data)
        #expect(snapshot.host == nil)
        #expect(snapshot.status == .error)
        #expect(snapshot.statusDetail == "connection refused")
        #expect(snapshot.ports.isEmpty)
    }

    @Test("port_entry fixture: omitted process/pid decode to nil")
    func portEntryOmittedOptionals() throws {
        let data = try fixtureData("port_entry.json")
        let entry = try decoder.decode(PortEntry.self, from: data)
        #expect(entry.remotePort == Port(8080))
        #expect(entry.process == nil)
        #expect(entry.pid == nil)
        #expect(entry.forward == .idle)
    }

    @Test("bare scalar fixtures: forward_id and port")
    func bareScalars() throws {
        let fid = try decoder.decode(ForwardId.self, from: try fixtureData("forward_id.json"))
        #expect(fid == ForwardId(7))
        let port = try decoder.decode(Port.self, from: try fixtureData("port.json"))
        #expect(port == Port(8080))
    }

    @Test("request fixture: flattened id + type")
    func requestFixture() throws {
        let req = try decoder.decode(Request.self, from: try fixtureData("request.json"))
        #expect(req.id == 42)
        #expect(req.body == .ping)
    }

    @Test("request side emits explicit null for absent local_port")
    func requestEmitsExplicitNull() throws {
        let body = RequestBody.startForward(remotePort: Port(3000), localPort: nil)
        let str = String(decoding: try encoder.encode(body), as: UTF8.self)
        #expect(str.contains("\"local_port\":null"))
    }

    @Test("set_config fixture decodes all fields")
    func setConfigFixture() throws {
        let body = try decoder.decode(RequestBody.self, from: try fixtureData("request_body_set_config.json"))
        guard case .setConfig(let alias, let secs, let reconnect) = body else {
            Issue.record("expected .setConfig")
            return
        }
        #expect(alias == "prod")
        #expect(secs == 5)
        #expect(reconnect == true)
    }
}
