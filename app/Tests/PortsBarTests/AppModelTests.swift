import Foundation
import Testing
@testable import PortsBar

/// Records every request sent through it, for intent tests.
final class RecordingSender: RequestSending, @unchecked Sendable {
    private let lock = NSLock()
    private var _requests: [Request] = []

    var requests: [Request] {
        lock.lock(); defer { lock.unlock() }
        return _requests
    }

    func send(_ request: Request) async throws {
        lock.lock(); _requests.append(request); lock.unlock()
    }
}

/// Isolated UserDefaults for tests.
private func makeDefaults() -> UserDefaults {
    let suite = "PortsBarTests.\(UUID().uuidString)"
    let d = UserDefaults(suiteName: suite)!
    return d
}

@MainActor
@Suite("AppModel state mapping")
struct AppModelStateTests {
    @Test("default state is disconnected and empty")
    func defaultState() {
        let model = AppModel(defaults: makeDefaults())
        #expect(model.state.connStatus == .disconnected)
        #expect(model.state.forwards.isEmpty)
        #expect(model.activeForwardCount == 0)
        #expect(model.isConnected == false)
    }

    @Test("applying a State snapshot replaces published state")
    func applyState() {
        let model = AppModel(defaults: makeDefaults())
        let snapshot = StateSnapshot(
            host: HostAlias("dev-desktop"),
            connStatus: .connected,
            forwards: [
                PortForward(id: ForwardId(1), remotePort: Port(3000),
                            status: .forwarding(localPort: Port(3000))),
                PortForward(id: ForwardId(2), remotePort: Port(5432), status: .idle),
            ]
        )
        model.apply(.state(snapshot))
        #expect(model.state.host == HostAlias("dev-desktop"))
        #expect(model.isConnected)
        #expect(model.state.forwards.count == 2)
    }

    @Test("badge counts only forwarding entries")
    func badgeCount() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.state(StateSnapshot(
            host: HostAlias("h"),
            connStatus: .connected,
            forwards: [
                PortForward(id: ForwardId(1), remotePort: Port(3000),
                            status: .forwarding(localPort: Port(3000))),
                PortForward(id: ForwardId(2), remotePort: Port(8080),
                            status: .forwarding(localPort: Port(8080))),
                PortForward(id: ForwardId(3), remotePort: Port(5432), status: .idle),
                PortForward(id: ForwardId(4), remotePort: Port(9000),
                            status: .error(message: "x")),
            ]
        )))
        #expect(model.activeForwardCount == 2)
    }

    @Test("conn_status_changed event updates only status")
    func connStatusEvent() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.state(StateSnapshot(host: HostAlias("h"), connStatus: .connecting, forwards: [])))
        model.apply(.event(.connStatusChanged(connStatus: .connected)))
        #expect(model.state.connStatus == .connected)
        #expect(model.state.host == HostAlias("h"))
    }

    @Test("forward_added appends, forward_removed removes")
    func forwardAddRemove() {
        let model = AppModel(defaults: makeDefaults())
        let fwd = PortForward(id: ForwardId(7), remotePort: Port(3000),
                              status: .forwarding(localPort: Port(3000)))
        model.apply(.event(.forwardAdded(forward: fwd)))
        #expect(model.state.forwards.count == 1)
        #expect(model.activeForwardCount == 1)
        model.apply(.event(.forwardRemoved(forwardId: ForwardId(7))))
        #expect(model.state.forwards.isEmpty)
    }

    @Test("forward_added with existing id replaces in place")
    func forwardAddedReplaces() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.event(.forwardAdded(forward: PortForward(
            id: ForwardId(1), remotePort: Port(3000), status: .idle))))
        model.apply(.event(.forwardAdded(forward: PortForward(
            id: ForwardId(1), remotePort: Port(3000),
            status: .forwarding(localPort: Port(3000))))))
        #expect(model.state.forwards.count == 1)
        #expect(model.activeForwardCount == 1)
    }

    @Test("ack with hosts populates host list")
    func ackHosts() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.ack(id: 3, error: nil,
                         hosts: [HostAlias("a"), HostAlias("b")]))
        #expect(model.hosts == [HostAlias("a"), HostAlias("b")])
    }

    @Test("ack with error raises an error toast")
    func ackError() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.ack(id: 7, error: .bindFailed(port: 8080, reason: "in use"), hosts: nil))
        #expect(model.toast?.isError == true)
        #expect(model.toast?.message.contains("8080") == true)
    }

    @Test("completed file transfer raises a success toast")
    func fileTransferToast() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.event(.fileTransfer(localPath: "/tmp/data.bin",
                                         bytesTransferred: 100, totalBytes: 100, done: true)))
        #expect(model.toast?.isError == false)
        #expect(model.toast?.message.contains("data.bin") == true)
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
        #expect(sender.requests.count == 1)
        guard case .startForward(let rp, let lp) = sender.requests[0].body else {
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
        guard case .startForward(_, let lp) = sender.requests[0].body else {
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
        guard case .stopForward(let rp) = sender.requests[0].body else {
            Issue.record("expected stop_forward")
            return
        }
        #expect(rp == Port(5432))
    }

    @Test("setHost persists and sends connect")
    func setHostIntent() async {
        let defaults = makeDefaults()
        let sender = RecordingSender()
        let model = AppModel(defaults: defaults, sender: sender)
        await model.setHost("prod-1")
        #expect(model.prefs.lastHost == "prod-1")
        guard case .connect(let host) = sender.requests[0].body else {
            Issue.record("expected connect")
            return
        }
        #expect(host == HostAlias("prod-1"))
    }

    @Test("sendFile sends send_file")
    func sendFileIntent() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.sendFile(localPath: "/tmp/x", remotePath: "/tmp")
        guard case .sendFile(let lp, let rp) = sender.requests[0].body else {
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
        #expect(sender.requests.count == 2)
        #expect(sender.requests[1].id > sender.requests[0].id)
    }

    @Test("pushConfig sends current prefs as DaemonConfig")
    func pushConfigIntent() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        model.prefs.autoReconnect = true
        model.prefs.autoRefreshSecs = 30
        await model.pushConfig()
        guard case .setConfig(let config) = sender.requests[0].body else {
            Issue.record("expected set_config")
            return
        }
        #expect(config.autoReconnect == true)
        #expect(config.autoRefreshSecs == 30)
    }
}

@Suite("Preferences persistence")
struct PreferencesTests {
    @Test("round-trips through UserDefaults")
    func roundTrip() {
        let d = makeDefaults()
        var p = Preferences()
        p.autoReconnect = false
        p.autoRefreshSecs = 45
        p.openBrowserOnForward = true
        p.launchAtLogin = true
        p.lastHost = "staging"
        p.save(to: d)
        let loaded = Preferences.load(from: d)
        #expect(loaded == p)
    }

    @Test("daemonConfig maps prefs to wire type")
    func daemonConfigMapping() {
        var p = Preferences()
        p.autoReconnect = true
        p.autoRefreshSecs = 10
        p.openBrowserOnForward = false
        #expect(p.daemonConfig.autoReconnect == true)
        #expect(p.daemonConfig.autoRefreshSecs == 10)
        #expect(p.daemonConfig.openBrowserOnForward == false)
    }
}
