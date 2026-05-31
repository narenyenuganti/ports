import Foundation
import Testing
@testable import PortsBar

/// Resolve the repo root from this source file's path.
/// #filePath is .../app/Tests/PortsBarTests/ProtocolDriftTests.swift
/// Drop the last 4 components (file, PortsBarTests, Tests, app) to reach the root.
private func repoRoot(file: String = #filePath) -> URL {
    var url = URL(fileURLWithPath: file)
    for _ in 0..<4 { url.deleteLastPathComponent() }
    return url
}

private func fixturesDir() -> URL {
    repoRoot().appendingPathComponent("tests/protocol_fixtures", isDirectory: true)
}

private func fixtureData(_ name: String) throws -> Data {
    let url = fixturesDir().appendingPathComponent(name)
    return try Data(contentsOf: url)
}

private let decoder = JSONDecoder()
private let encoder = JSONEncoder()

/// Categorize a fixture filename to the Swift type it should decode into.
private enum FixtureKind {
    case connStatus
    case error
    case daemonMessage
    case requestBody
    case request
}

private func kind(for name: String) -> FixtureKind {
    if name.hasPrefix("conn_status_") { return .connStatus }
    if name.hasPrefix("error_") { return .error }
    if name.hasPrefix("daemon_message_") { return .daemonMessage }
    if name.hasPrefix("request_body_") { return .requestBody }
    // request_set_host.json, request_start_forward.json, request_with_id.json
    if name.hasPrefix("request_") { return .request }
    fatalError("uncategorized fixture: \(name)")
}

/// Decode + round-trip a fixture by its categorized kind. Throws on failure.
private func decodeAndRoundTrip(_ name: String) throws {
    let data = try fixtureData(name)
    switch kind(for: name) {
    case .connStatus:
        let v = try decoder.decode(ConnStatus.self, from: data)
        let re = try encoder.encode(v)
        let v2 = try decoder.decode(ConnStatus.self, from: re)
        #expect(v == v2)
    case .error:
        let v = try decoder.decode(ProtocolError.self, from: data)
        let re = try encoder.encode(v)
        let v2 = try decoder.decode(ProtocolError.self, from: re)
        #expect(v == v2)
    case .daemonMessage:
        let v = try decoder.decode(DaemonMessage.self, from: data)
        let re = try encoder.encode(v)
        let v2 = try decoder.decode(DaemonMessage.self, from: re)
        #expect(v == v2)
    case .requestBody:
        let v = try decoder.decode(RequestBody.self, from: data)
        let re = try encoder.encode(v)
        let v2 = try decoder.decode(RequestBody.self, from: re)
        #expect(v == v2)
    case .request:
        let v = try decoder.decode(Request.self, from: data)
        let re = try encoder.encode(v)
        let v2 = try decoder.decode(Request.self, from: re)
        #expect(v == v2)
    }
}

@Suite("Protocol drift")
struct ProtocolDriftTests {
    static let allFixtures = [
        "conn_status_connected.json",
        "conn_status_connecting.json",
        "conn_status_disconnected.json",
        "conn_status_error.json",
        "daemon_message_ack_error.json",
        "daemon_message_ack_hosts.json",
        "daemon_message_ack_ok.json",
        "daemon_message_event_conn_status.json",
        "daemon_message_event_file_transfer.json",
        "daemon_message_event_forward_added.json",
        "daemon_message_event_forward_removed.json",
        "daemon_message_event_log.json",
        "daemon_message_state.json",
        "daemon_message_state_empty.json",
        "error_bind_failed.json",
        "error_connection_failed.json",
        "error_forward_not_found.json",
        "error_host_not_found.json",
        "error_internal.json",
        "error_invalid_request.json",
        "error_io.json",
        "error_not_connected.json",
        "error_ssh.json",
        "error_timeout.json",
        "request_body_connect.json",
        "request_body_disconnect.json",
        "request_body_list_hosts.json",
        "request_body_send_file.json",
        "request_body_set_config.json",
        "request_body_start_forward.json",
        "request_body_stop_forward.json",
        "request_set_host.json",
        "request_start_forward.json",
        "request_with_id.json",
    ]

    @Test("repo root resolves and fixtures dir has all 34 files")
    func fixturesPresent() throws {
        let dir = fixturesDir()
        let contents = try FileManager.default.contentsOfDirectory(
            at: dir, includingPropertiesForKeys: nil
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
        #expect(snapshot.host == HostAlias("dev-desktop"))
        #expect(snapshot.connStatus == .connected)
        #expect(snapshot.forwards.count == 2)
        #expect(snapshot.forwards[0].status == .forwarding(localPort: Port(3000)))
        // Second forward is idle with process/pid present.
        #expect(snapshot.forwards[1].status == .idle)
        #expect(snapshot.forwards[1].process == "postgres")
        #expect(snapshot.forwards[1].pid == 4242)
        // First forward has nil optionals (omitted by daemon).
        #expect(snapshot.forwards[0].process == nil)
        #expect(snapshot.forwards[0].pid == nil)
        #expect(snapshot.forwards[0].statusDetail == nil)
    }

    @Test("start_forward request body decodes local_port Some")
    func startForwardSome() throws {
        let data = try fixtureData("request_body_start_forward.json")
        let body = try decoder.decode(RequestBody.self, from: data)
        guard case .startForward(let remotePort, let localPort) = body else {
            Issue.record("expected .startForward")
            return
        }
        #expect(remotePort == Port(3000))
        #expect(localPort == Port(3000))
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

    @Test("request_start_forward fixture (full Request, null local_port)")
    func requestStartForwardNull() throws {
        let data = try fixtureData("request_start_forward.json")
        let req = try decoder.decode(Request.self, from: data)
        #expect(req.id == 2)
        guard case .startForward(let remotePort, let localPort) = req.body else {
            Issue.record("expected .startForward")
            return
        }
        #expect(remotePort == Port(3000))
        #expect(localPort == nil)
    }

    @Test("ack_error fixture decodes to .ack with bindFailed")
    func ackErrorFixture() throws {
        let data = try fixtureData("daemon_message_ack_error.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 7)
        #expect(hosts == nil)
        #expect(error == .bindFailed(port: 8080, reason: "address in use"))
    }

    @Test("ack_hosts fixture decodes host list")
    func ackHostsFixture() throws {
        let data = try fixtureData("daemon_message_ack_hosts.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 3)
        #expect(error == nil)
        #expect(hosts == [HostAlias("dev-desktop"), HostAlias("prod-1"), HostAlias("staging")])
    }

    @Test("ack_ok fixture: both optionals nil")
    func ackOkFixture() throws {
        let data = try fixtureData("daemon_message_ack_ok.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 1)
        #expect(error == nil)
        #expect(hosts == nil)
    }

    @Test("file_transfer event decodes")
    func fileTransferEvent() throws {
        let data = try fixtureData("daemon_message_event_file_transfer.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .event(.fileTransfer(let path, let sent, let total, let done)) = msg else {
            Issue.record("expected .event(.fileTransfer)")
            return
        }
        #expect(path == "/tmp/data.bin")
        #expect(sent == 1024)
        #expect(total == 4096)
        #expect(done == false)
    }

    @Test("empty state fixture: host nil, disconnected")
    func emptyStateFixture() throws {
        let data = try fixtureData("daemon_message_state_empty.json")
        let msg = try decoder.decode(DaemonMessage.self, from: data)
        guard case .state(let snapshot) = msg else {
            Issue.record("expected .state")
            return
        }
        #expect(snapshot.host == nil)
        #expect(snapshot.connStatus == .disconnected)
        #expect(snapshot.forwards.isEmpty)
    }

    @Test("request side emits explicit null for absent local_port")
    func requestEmitsExplicitNull() throws {
        let body = RequestBody.startForward(remotePort: Port(3000), localPort: nil)
        let data = try encoder.encode(body)
        let str = String(decoding: data, as: UTF8.self)
        #expect(str.contains("\"local_port\":null"))
    }
}
