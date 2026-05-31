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

/// Records every URL opened through the injectable open-browser seam.
/// @MainActor-isolated: opens only ever happen on the main actor (AppModel).
@MainActor
final class RecordingOpener: URLOpening {
    private(set) var opened: [URL] = []
    func open(_ url: URL) { opened.append(url) }
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
        let snapshot = PortsState(
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

    @Test("connected snapshot persists host for next launch")
    func remembersConnectedHost() {
        let defaults = makeDefaults()
        let model = AppModel(defaults: defaults)
        #expect(model.prefs.host == "")

        model.apply(.state(PortsState(
            host: "dev-desktop", status: .connected, statusDetail: nil, ports: [])))

        #expect(model.prefs.host == "dev-desktop")
        // A fresh model reading the same defaults restores it.
        #expect(AppModel(defaults: defaults).prefs.host == "dev-desktop")
    }

    @Test("disconnected snapshot does not overwrite remembered host")
    func keepsHostWhenDisconnected() {
        let defaults = makeDefaults()
        let model = AppModel(defaults: defaults)
        model.apply(.state(PortsState(
            host: "dev-desktop", status: .connected, statusDetail: nil, ports: [])))
        model.apply(.state(PortsState(
            host: nil, status: .disconnected, statusDetail: nil, ports: [])))
        #expect(model.prefs.host == "dev-desktop")
    }

    @Test("badge counts only forwarding entries")
    func badgeCount() {
        let model = AppModel(defaults: makeDefaults())
        model.apply(.state(PortsState(
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
        model.apply(.state(PortsState(
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

    @Test("open-on-forward ON: idle->forwarding opens exactly one URL")
    func openOnForwardOpensOnTransition() {
        let defaults = makeDefaults()
        let opener = RecordingOpener()
        let model = AppModel(defaults: defaults, opener: opener)
        model.prefs.openBrowserOnForward = true

        // Prior state: port idle.
        model.apply(.state(PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .idle)]
        )))
        #expect(opener.opened.isEmpty)

        // Transition: idle -> forwarding(localPort: 13000).
        model.apply(.state(PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000)))]
        )))
        #expect(opener.opened == [URL(string: "http://localhost:13000")!])
    }

    @Test("open-on-forward ON: still-forwarding push opens nothing (no duplicate)")
    func openOnForwardNoDuplicate() {
        let opener = RecordingOpener()
        let model = AppModel(defaults: makeDefaults(), opener: opener)
        model.prefs.openBrowserOnForward = true

        let forwarding = PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000)))]
        )
        model.apply(.state(forwarding))
        #expect(opener.opened.count == 1)

        // Same port still forwarding on a subsequent refresh push: no re-open.
        model.apply(.state(forwarding))
        #expect(opener.opened.count == 1)
    }

    @Test("open-on-forward OFF: idle->forwarding opens nothing")
    func openOnForwardOffOpensNothing() {
        let opener = RecordingOpener()
        let model = AppModel(defaults: makeDefaults(), opener: opener)
        model.prefs.openBrowserOnForward = false

        model.apply(.state(PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .idle)]
        )))
        model.apply(.state(PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000)))]
        )))
        #expect(opener.opened.isEmpty)
    }

    @Test("open-on-forward ON: two newly-forwarded ports open two URLs")
    func openOnForwardTwoPorts() {
        let opener = RecordingOpener()
        let model = AppModel(defaults: makeDefaults(), opener: opener)
        model.prefs.openBrowserOnForward = true

        model.apply(.state(PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [
                PortEntry(remotePort: Port(3000), forward: .idle),
                PortEntry(remotePort: Port(8080), forward: .idle),
            ]
        )))
        model.apply(.state(PortsState(
            host: "h", status: .connected, statusDetail: nil,
            ports: [
                PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000))),
                PortEntry(remotePort: Port(8080), forward: .forwarding(localPort: Port(18080))),
            ]
        )))
        #expect(Set(opener.opened) == [
            URL(string: "http://localhost:13000")!,
            URL(string: "http://localhost:18080")!,
        ])
        #expect(opener.opened.count == 2)
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

    @Test("activating an idle port starts forwarding it")
    func activateIdlePortStartsForwarding() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        model.apply(.state(PortsState(
            host: "h",
            status: .connected,
            statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .idle)]
        )))

        await model.activate(remotePort: 3000)

        let requests = await sender.log.requests
        #expect(requests.count == 1)
        guard case .startForward(let remotePort, let localPort) = requests[0].body else {
            Issue.record("expected start_forward")
            return
        }
        #expect(remotePort == Port(3000))
        #expect(localPort == nil)
    }

    @Test("activating an already-forwarding port does not send another start")
    func activateForwardingPortDoesNotRestart() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        model.apply(.state(PortsState(
            host: "h",
            status: .connected,
            statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000)))]
        )))

        await model.activate(remotePort: 3000)

        let requests = await sender.log.requests
        #expect(requests.isEmpty)
    }

    @Test("repeated forward clicks while pending send one request")
    func repeatedForwardClicksWhilePendingSendOneRequest() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.forward(remotePort: 3000)
        await model.forward(remotePort: 3000)
        let requests = await sender.log.requests
        #expect(requests.count == 1)
    }

    @Test("forward click is accepted again after daemon state resolves pending request")
    func forwardClickAcceptedAfterStateResolution() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.forward(remotePort: 3000)
        model.apply(.state(PortsState(
            host: "h",
            status: .connected,
            statusDetail: nil,
            ports: [PortEntry(remotePort: Port(3000), forward: .forwarding(localPort: Port(13000)))]
        )))
        await model.forward(remotePort: 3000)
        let requests = await sender.log.requests
        #expect(requests.count == 2)
    }

    @Test("repeated stop clicks while pending send one request")
    func repeatedStopClicksWhilePendingSendOneRequest() async {
        let sender = RecordingSender()
        let model = AppModel(defaults: makeDefaults(), sender: sender)
        await model.stop(remotePort: 5432)
        await model.stop(remotePort: 5432)
        let requests = await sender.log.requests
        #expect(requests.count == 1)
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
