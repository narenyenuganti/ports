import Foundation
import Testing
@testable import PortsBar

/// Resolves the repo root from this source file's path. `#filePath` points at
/// `<repo>/app/Tests/PortsBarTests/ProtocolDriftTests.swift`; dropping the last
/// three path components yields `<repo>`.
private func repoRoot(file: String = #filePath) -> URL {
    URL(fileURLWithPath: file)
        .deletingLastPathComponent() // PortsBarTests/
        .deletingLastPathComponent() // Tests/
        .deletingLastPathComponent() // app/
        .deletingLastPathComponent() // <repo>/
}

private func fixturesDir() -> URL {
    repoRoot().appendingPathComponent("tests/protocol_fixtures", isDirectory: true)
}

private func fixtureData(_ name: String) throws -> Data {
    let url = fixturesDir().appendingPathComponent("\(name).json")
    return try Data(contentsOf: url)
}

/// Decode -> encode -> decode again and assert the value is stable.
private func assertRoundTrip<T: Codable & Equatable>(_ type: T.Type, data: Data) throws -> T {
    let decoder = JSONDecoder()
    let value = try decoder.decode(T.self, from: data)
    let reencoded = try JSONEncoder().encode(value)
    let again = try decoder.decode(T.self, from: reencoded)
    #expect(value == again, "round-trip instability for \(T.self)")
    return value
}

struct ProtocolDriftTests {
    // MARK: - Targeted assertions on key fixtures

    @Test func decodesDaemonStateFixture() throws {
        let data = try fixtureData("daemon_message_state")
        let msg = try assertRoundTrip(DaemonMessage.self, data: data)
        guard case .state(let snapshot) = msg else {
            Issue.record("expected .state, got \(msg)")
            return
        }
        // The committed fixture host is "prod".
        #expect(snapshot.host == "prod")
        #expect(snapshot.status == .connected)
        #expect(snapshot.ports.count == 1)
        let first = try #require(snapshot.ports.first)
        #expect(first.remotePort == Port(5432))
        #expect(first.process == "postgres")
        #expect(first.pid == 1234)
        #expect(first.forward == .forwarding(localPort: Port(15432)))
    }

    @Test func startForwardWithLocalPortDecodes() throws {
        let data = try fixtureData("request_body_start_forward")
        let body = try assertRoundTrip(RequestBody.self, data: data)
        guard case .startForward(let remotePort, let localPort) = body else {
            Issue.record("expected .startForward, got \(body)")
            return
        }
        #expect(remotePort == Port(5432))
        // The committed fixture carries an explicit local_port (Some case).
        #expect(localPort == Port(15432))
    }

    @Test func startForwardWithNullLocalPortDecodesToNil() throws {
        // Synthetic payload exercising the missing/null asymmetry on the
        // request side: explicit null must decode to nil.
        let json = #"{"id":1,"type":"start_forward","remote_port":3000,"local_port":null}"#
        let data = Data(json.utf8)
        let req = try JSONDecoder().decode(Request.self, from: data)
        #expect(req.id == 1)
        guard case .startForward(let remotePort, let localPort) = req.body else {
            Issue.record("expected .startForward, got \(req.body)")
            return
        }
        #expect(remotePort == Port(3000))
        #expect(localPort == nil)
    }

    @Test func startForwardWithMissingLocalPortDecodesToNil() throws {
        // A missing key must also decode to nil (decodeIfPresent behavior).
        let json = #"{"id":2,"type":"start_forward","remote_port":3000}"#
        let data = Data(json.utf8)
        let req = try JSONDecoder().decode(Request.self, from: data)
        guard case .startForward(_, let localPort) = req.body else {
            Issue.record("expected .startForward")
            return
        }
        #expect(localPort == nil)
    }

    @Test func startForwardEncodesLocalPortAsExplicitNull() throws {
        // CRITICAL ASYMMETRY: the encoder must emit `local_port` as explicit
        // null (not omit) when absent, matching the Rust request side.
        let body = RequestBody.startForward(remotePort: Port(3000), localPort: nil)
        let encoded = try JSONEncoder().encode(body)
        let str = String(decoding: encoded, as: UTF8.self)
        #expect(str.contains("\"local_port\":null"), "expected explicit null, got: \(str)")
    }

    @Test func ackErrorFixtureDecodesNotConnected() throws {
        let data = try fixtureData("daemon_message_ack_error")
        let msg = try assertRoundTrip(DaemonMessage.self, data: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack, got \(msg)")
            return
        }
        #expect(id == 8)
        #expect(error == .notConnected)
        #expect(hosts == nil)
    }

    @Test func ackBindFailedDecodes() throws {
        // The task references a bindFailed ack; the committed ack_error fixture
        // is not_connected, so exercise bindFailed via a synthetic ack payload
        // and the dedicated error fixture.
        let json = #"{"type":"ack","id":3,"error":{"kind":"bind_failed","port":8080,"detail":"address in use"}}"#
        let msg = try JSONDecoder().decode(DaemonMessage.self, from: Data(json.utf8))
        guard case .ack(_, let error, _) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(error == .bindFailed(port: Port(8080), detail: "address in use"))
    }

    @Test func ackHostsFixtureDecodes() throws {
        let data = try fixtureData("daemon_message_ack_hosts")
        let msg = try assertRoundTrip(DaemonMessage.self, data: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 9)
        #expect(error == nil)
        #expect(hosts == ["prod", "staging"])
    }

    @Test func ackOkFixtureDecodes() throws {
        let data = try fixtureData("daemon_message_ack_ok")
        let msg = try assertRoundTrip(DaemonMessage.self, data: data)
        guard case .ack(let id, let error, let hosts) = msg else {
            Issue.record("expected .ack")
            return
        }
        #expect(id == 7)
        #expect(error == nil)
        #expect(hosts == nil)
    }

    @Test func eventFixtureDecodes() throws {
        let data = try fixtureData("daemon_message_event")
        let msg = try assertRoundTrip(DaemonMessage.self, data: data)
        guard case .event(let event) = msg else {
            Issue.record("expected .event")
            return
        }
        #expect(event == .fileTransfer(ok: true, detail: "uploaded 3 files"))
    }

    @Test func requestFixtureFlattensIDAndType() throws {
        let data = try fixtureData("request")
        let req = try assertRoundTrip(Request.self, data: data)
        #expect(req.id == 42)
        #expect(req.body == .ping)
    }

    @Test func stateSnapshotFixtureWithNullHost() throws {
        let data = try fixtureData("state_snapshot")
        let snapshot = try assertRoundTrip(StateSnapshot.self, data: data)
        #expect(snapshot.host == nil)
        #expect(snapshot.status == .error)
        #expect(snapshot.statusDetail == "connection refused")
        #expect(snapshot.ports.isEmpty)
    }

    @Test func portEntryFixtureOmitsOptionalFields() throws {
        let data = try fixtureData("port_entry")
        let entry = try assertRoundTrip(PortEntry.self, data: data)
        #expect(entry.remotePort == Port(8080))
        #expect(entry.process == nil)
        #expect(entry.pid == nil)
        #expect(entry.forward == .idle)
    }

    @Test func bareScalarFixtures() throws {
        #expect(try assertRoundTrip(ForwardId.self, data: fixtureData("forward_id")) == ForwardId(7))
        #expect(try assertRoundTrip(Port.self, data: fixtureData("port")) == Port(8080))
        #expect(try assertRoundTrip(ConnStatus.self, data: fixtureData("conn_status_connected")) == .connected)
        #expect(try assertRoundTrip(ConnStatus.self, data: fixtureData("conn_status_connecting")) == .connecting)
        #expect(try assertRoundTrip(ConnStatus.self, data: fixtureData("conn_status_disconnected")) == .disconnected)
        #expect(try assertRoundTrip(ConnStatus.self, data: fixtureData("conn_status_error")) == .error)
    }

    @Test func forwardStateFixtures() throws {
        #expect(try assertRoundTrip(ForwardState.self, data: fixtureData("forward_state_idle")) == .idle)
        #expect(try assertRoundTrip(ForwardState.self, data: fixtureData("forward_state_forwarding"))
            == .forwarding(localPort: Port(9000)))
        #expect(try assertRoundTrip(ForwardState.self, data: fixtureData("forward_state_error"))
            == .error(detail: "remote closed"))
    }

    @Test func errorFixtures() throws {
        #expect(try assertRoundTrip(ProtocolError.self, data: fixtureData("error_not_connected")) == .notConnected)
        #expect(try assertRoundTrip(ProtocolError.self, data: fixtureData("error_connect_failed"))
            == .connectFailed(detail: "handshake timed out"))
        #expect(try assertRoundTrip(ProtocolError.self, data: fixtureData("error_bind_failed"))
            == .bindFailed(port: Port(8080), detail: "address in use"))
        #expect(try assertRoundTrip(ProtocolError.self, data: fixtureData("error_unknown_host"))
            == .unknownHost(alias: "prod"))
        #expect(try assertRoundTrip(ProtocolError.self, data: fixtureData("error_send_file_failed"))
            == .sendFileFailed(detail: "permission denied"))
        #expect(try assertRoundTrip(ProtocolError.self, data: fixtureData("error_bad_request"))
            == .badRequest(detail: "unknown type"))
    }

    @Test func daemonEventFixture() throws {
        #expect(try assertRoundTrip(DaemonEvent.self, data: fixtureData("daemon_event_file_transfer"))
            == .fileTransfer(ok: false, detail: "transfer aborted"))
    }

    @Test func requestBodyFixtures() throws {
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_connect")) == .connect)
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_disconnect")) == .disconnect)
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_refresh")) == .refresh)
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_list_hosts")) == .listHosts)
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_ping")) == .ping)
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_shutdown")) == .shutdown)
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_stop_forward"))
            == .stopForward(remotePort: Port(5432)))
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_send_file"))
            == .sendFile(localPath: "/tmp/data.bin", remotePath: "/srv/data.bin"))
        #expect(try assertRoundTrip(RequestBody.self, data: fixtureData("request_body_set_config"))
            == .setConfig(hostAlias: "prod", refreshSecs: 5, autoReconnect: true))
    }

    // MARK: - Exhaustive: every fixture decodes into the right type

    @Test func everyFixtureDecodes() throws {
        // Map each fixture filename to a decode closure for its concrete type.
        // Covers all 34 committed fixtures; any new or renamed fixture that is
        // not listed here fails the explicit count assertion below.
        let decoders: [String: (Data) throws -> Void] = [
            "conn_status_connected": { _ = try JSONDecoder().decode(ConnStatus.self, from: $0) },
            "conn_status_connecting": { _ = try JSONDecoder().decode(ConnStatus.self, from: $0) },
            "conn_status_disconnected": { _ = try JSONDecoder().decode(ConnStatus.self, from: $0) },
            "conn_status_error": { _ = try JSONDecoder().decode(ConnStatus.self, from: $0) },
            "daemon_event_file_transfer": { _ = try JSONDecoder().decode(DaemonEvent.self, from: $0) },
            "daemon_message_ack_error": { _ = try JSONDecoder().decode(DaemonMessage.self, from: $0) },
            "daemon_message_ack_hosts": { _ = try JSONDecoder().decode(DaemonMessage.self, from: $0) },
            "daemon_message_ack_ok": { _ = try JSONDecoder().decode(DaemonMessage.self, from: $0) },
            "daemon_message_event": { _ = try JSONDecoder().decode(DaemonMessage.self, from: $0) },
            "daemon_message_state": { _ = try JSONDecoder().decode(DaemonMessage.self, from: $0) },
            "error_bad_request": { _ = try JSONDecoder().decode(ProtocolError.self, from: $0) },
            "error_bind_failed": { _ = try JSONDecoder().decode(ProtocolError.self, from: $0) },
            "error_connect_failed": { _ = try JSONDecoder().decode(ProtocolError.self, from: $0) },
            "error_not_connected": { _ = try JSONDecoder().decode(ProtocolError.self, from: $0) },
            "error_send_file_failed": { _ = try JSONDecoder().decode(ProtocolError.self, from: $0) },
            "error_unknown_host": { _ = try JSONDecoder().decode(ProtocolError.self, from: $0) },
            "forward_id": { _ = try JSONDecoder().decode(ForwardId.self, from: $0) },
            "forward_state_error": { _ = try JSONDecoder().decode(ForwardState.self, from: $0) },
            "forward_state_forwarding": { _ = try JSONDecoder().decode(ForwardState.self, from: $0) },
            "forward_state_idle": { _ = try JSONDecoder().decode(ForwardState.self, from: $0) },
            "port": { _ = try JSONDecoder().decode(Port.self, from: $0) },
            "port_entry": { _ = try JSONDecoder().decode(PortEntry.self, from: $0) },
            "request": { _ = try JSONDecoder().decode(Request.self, from: $0) },
            "request_body_connect": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_disconnect": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_list_hosts": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_ping": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_refresh": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_send_file": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_set_config": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_shutdown": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_start_forward": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "request_body_stop_forward": { _ = try JSONDecoder().decode(RequestBody.self, from: $0) },
            "state_snapshot": { _ = try JSONDecoder().decode(StateSnapshot.self, from: $0) },
        ]

        // Assert we cover exactly the files on disk (drift guard).
        let onDisk = try FileManager.default
            .contentsOfDirectory(at: fixturesDir(), includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "json" }
            .map { $0.deletingPathExtension().lastPathComponent }
            .sorted()
        #expect(onDisk.count == 34, "expected 34 fixtures, found \(onDisk.count)")
        #expect(Set(onDisk) == Set(decoders.keys), "fixture set drift: \(Set(onDisk).symmetricDifference(Set(decoders.keys)))")

        for name in onDisk {
            let decode = try #require(decoders[name], "no decoder mapped for fixture \(name)")
            do {
                try decode(fixtureData(name))
            } catch {
                Issue.record("fixture \(name).json failed to decode: \(error)")
            }
        }
    }
}
