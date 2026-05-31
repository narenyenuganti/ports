import Foundation
import Testing
@testable import PortsBarCore

/// Records every request sent through it. Uses an actor-backed buffer accessed
/// only from the @MainActor test, avoiding NSLock in async contexts.
actor RequestLog {
    private(set) var requests: [Request] = []
    func append(_ r: Request) { requests.append(r) }
}

final class RecordingSender: RequestSending {
    let log = RequestLog()
    func send(_ request: Request) async throws {
        await log.append(request)
    }
}

private func makeDefaults() -> UserDefaults {
    UserDefaults(suiteName: "PortsBarTests.\(UUID().uuidString)")!
}

@MainActor
@Suite("AppModel state mapping")
struct AppModelStateTests {
    @Test("default state is disconnected and empty")
    func defaultState() {
        let model = AppModel(defaults: makeDefaults())
        #expect(model.state.status == .disconnected)
        #expect(model.state.ports.isEmpty)
        #expect(model.activeForwardCount == 0)
        #expect(model.isConnected == false)
    }

    @Test("applying a State snapshot replaces published state")
    func applyState() {
        let model = AppModel(defaults: makeDefaults())
        let snapshot = StateSnapshot(
            host: "dev-desktop",
            status: .connected,
            statusDetail: nil,
            ports: [
                PortEntry(remotePort: Port(3000), process: "next", pid: 10,
                          forward: .forwarding(localPort: Port(3000))),
                PortEntry(remotePort: Port(5432), forward: .idle),
            ]
        )
        model.apply(.state(snapshot))
        #expect(model.state.host == "dev-desktop")
        #expect(model.isConnected)
        #expect(model.state.ports.count == 2)
    }

    @Test("badge counts only forwarding entries")
    func badgeCount() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.state(StateSnapshot(
            host: "h", status: .connected, statusDetail: nil,
            ports: [
                PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(3000))),
                PortEntry(remotePort: Port(8080), forward: .forwarding(localPort: Port(8080))),
                PortEntry(remotePort: Port(5432), forward: .idle),
                PortEntry(remotePort: Port(9000), forward: .error(detail: "x")),
            ]
        )))
        #expect(model.activeForwardCount == 2)
    }

    @Test("localPort(forRemote:) returns the bound local port")
    func localPortLookup() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.state(StateSnapshot(
            host: "h", status: .connected, statusDetail: nil,
            ports: [
                PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000))),
                PortEntry(remotePort: Port(8080), forward: .idle),
            ]
        )))
        #expect(model.localPort(forRemote: 3000) == 13000)
        #expect(model.localPort(forRemote: 8080) == nil)
    }

    @Test("ack with hosts populates host list")
    func ackHosts() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.ack(id: 9, error: nil, hosts: ["a", "b"]))
        #expect(model.hosts == ["a", "b"])
    }

    @Test("ack with error raises an error toast")
    func ackError() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.ack(id: 8, error: .bindFailed(port: Port(8080), detail: "in use"), hosts: nil))
        #expect(model.toast?.isError == true)
        #expect(model.toast?.message.contains("8080") == true)
    }

    @Test("successful file transfer raises a success toast")
    func fileTransferOkToast() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.event(.fileTransfer(ok: true, detail: "uploaded data.bin")))
        #expect(model.toast?.isError == false)
        #expect(model.toast?.message == "uploaded data.bin")
    }

    @Test("failed file transfer raises an error toast")
    func fileTransferFailToast() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.event(.fileTransfer(ok: false, detail: "permission denied")))
        #expect(model.toast?.isError == true)
    }
}

@MainActor
@Suite("AppModel intents")
struct AppModelIntentTests {
    @Test("forward sends start_forward with mapped ports")
    func forwardIntent() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.forward(remotePort: 3000, localPort: 8080)
        let requests = await sender.log.requests
        #expect(requests.count == 1)
        guard case .startForward(let rp, let lp) = requests[0].body else {
            Issue.record("expected start_forward")
            return
        }
        #expect(rp == Port(3000))
        #expect(lp == Port(8080))
    }

    @Test("forward without local port sends nil local_port")
    func forwardNoLocal() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.forward(remotePort: 3000)
        let requests = await sender.log.requests
        guard case .startForward(_, let lp) = requests[0].body else {
            Issue.record("expected start_forward")
            return
        }
        #expect(lp == nil)
    }

    @Test("stop sends stop_forward")
    func stopIntent() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.stop(remotePort: 5432)
        let requests = await sender.log.requests
        guard case .stopForward(let rp) = requests[0].body else {
            Issue.record("expected stop_forward")
            return
        }
        #expect(rp == Port(5432))
    }

    @Test("setHost persists, pushes config, then connects")
    func setHostIntent() async {
        let defaults = makeDefaults()
        let sender = RecordingSender()
        let model = AppModel(defaults: defaults, sender: sender)
        await model.setHost("prod-1")
        #expect(model.prefs.host == "prod-1")
        let requests = await sender.log.requests
        #expect(requests.count == 2)
        guard case .setConfig(let alias, _, _) = requests[0].body else {
            Issue.record("expected set_config first")
            return
        }
        #expect(alias == "prod-1")
        #expect(requests[1].body == .connect)
    }

    @Test("sendFile sends send_file")
    func sendFileIntent() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.sendFile(localPath: "/tmp/x", remotePath: "/tmp")
        let requests = await sender.log.requests
        guard case .sendFile(let lp, let rp) = requests[0].body else {
            Issue.record("expected send_file")
            return
        }
        #expect(lp == "/tmp/x")
        #expect(rp == "/tmp")
    }

    @Test("request ids are monotonic")
    func monotonicIds() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.refresh()
        await model.refresh()
        let requests = await sender.log.requests
        #expect(requests.count == 2)
        #expect(requests[1].id > requests[0].id)
    }

    @Test("pushConfig sends current prefs as set_config")
    func pushConfigIntent() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        model.prefs.host = "prod"
        model.prefs.autoReconnect = true
        model.prefs.autoRefreshSecs = 30
        await model.pushConfig()
        let requests = await sender.log.requests
        guard case .setConfig(let alias, let secs, let reconnect) = requests[0].body else {
            Issue.record("expected set_config")
            return
        }
        #expect(alias == "prod")
        #expect(secs == 30)
        #expect(reconnect == true)
    }
}

@Suite("Preferences persistence")
struct PreferencesTests {
    @Test("round-trips through UserDefaults")
    func roundTrip() {
        let d = makeDefaults()
        var p = Preferences()
        p.host = "staging"
        p.autoReconnect = false
        p.autoRefreshSecs = 45
        p.openBrowserOnForward = true
        p.launchAtLogin = true
        p.save(to: d)
        #expect(Preferences.load(from: d) == p)
    }
}
