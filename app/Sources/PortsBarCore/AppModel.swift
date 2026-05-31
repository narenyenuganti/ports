import AppKit
import Foundation
import SwiftUI

// MARK: - Request sending abstraction
//
// Abstracts the daemon transport so AppModel is testable without a real socket.
// DaemonClient conforms via an extension below.

public protocol RequestSending: Sendable {
    func send(_ request: Request) async throws
}

extension DaemonClient: RequestSending {}

// MARK: - URL opening abstraction
//
// Abstracts NSWorkspace so the open-browser-on-forward behavior is testable
// without launching a real browser. Production uses NSWorkspace via the default
// init below. @MainActor because opening is always driven from the main actor.

@MainActor
public protocol URLOpening {
    func open(_ url: URL)
}

/// Production opener: hands the URL to the system default handler.
@MainActor
public struct WorkspaceURLOpener: URLOpening {
    public init() {}
    public func open(_ url: URL) { NSWorkspace.shared.open(url) }
}

// MARK: - Toast
//
// Transient user-facing message (Ack errors, file-transfer results, etc.).

public struct Toast: Identifiable, Equatable, Sendable {
    public let id = UUID()
    public var message: String
    public var isError: Bool
}

private enum PendingPortIntent: Equatable, Sendable {
    case start
    case stop
}

// MARK: - Preferences
//
// Persisted to UserDefaults; the daemon-relevant subset is pushed via SetConfig.

public struct Preferences: Equatable, Sendable {
    public var host: String = ""
    public var autoReconnect: Bool = true
    public var autoRefreshSecs: UInt64 = 0
    public var openBrowserOnForward: Bool = false
    public var launchAtLogin: Bool = false

    private enum Keys {
        static let host = "host"
        static let autoReconnect = "autoReconnect"
        static let autoRefreshSecs = "autoRefreshSecs"
        static let openBrowserOnForward = "openBrowserOnForward"
        static let launchAtLogin = "launchAtLogin"
    }

    static func load(from defaults: UserDefaults) -> Preferences {
        var p = Preferences()
        p.host = defaults.string(forKey: Keys.host) ?? ""
        if defaults.object(forKey: Keys.autoReconnect) != nil {
            p.autoReconnect = defaults.bool(forKey: Keys.autoReconnect)
        }
        p.autoRefreshSecs = UInt64(max(0, defaults.integer(forKey: Keys.autoRefreshSecs)))
        p.openBrowserOnForward = defaults.bool(forKey: Keys.openBrowserOnForward)
        p.launchAtLogin = defaults.bool(forKey: Keys.launchAtLogin)
        return p
    }

    func save(to defaults: UserDefaults) {
        defaults.set(host, forKey: Keys.host)
        defaults.set(autoReconnect, forKey: Keys.autoReconnect)
        defaults.set(Int(autoRefreshSecs), forKey: Keys.autoRefreshSecs)
        defaults.set(openBrowserOnForward, forKey: Keys.openBrowserOnForward)
        defaults.set(launchAtLogin, forKey: Keys.launchAtLogin)
    }
}

// MARK: - AppModel

@MainActor
public final class AppModel: ObservableObject {
    /// Latest daemon state. Defaults to disconnected/empty.
    @Published public var state = PortsState(host: nil, status: .disconnected, statusDetail: nil, ports: [])
    /// Persisted preferences.
    @Published public var prefs: Preferences
    /// Available SSH hosts (from ListHosts Ack).
    @Published public var hosts: [String] = []
    /// Active toast, if any.
    @Published public var toast: Toast?

    private let defaults: UserDefaults
    private var sender: (any RequestSending)?
    private let opener: any URLOpening
    private var nextRequestId: UInt64 = 1
    @Published private var pendingPortIntents: [UInt16: PendingPortIntent] = [:]
    private var pendingRequestPorts: [UInt64: UInt16] = [:]

    public init(
        defaults: UserDefaults = .standard,
        sender: (any RequestSending)? = nil,
        opener: any URLOpening = WorkspaceURLOpener()
    ) {
        self.defaults = defaults
        self.sender = sender
        self.opener = opener
        self.prefs = Preferences.load(from: defaults)
    }

    /// Open `http://localhost:<port>` via the injected opener (used by views too).
    func openLocalPort(_ localPort: UInt16) {
        guard let url = URL(string: "http://localhost:\(localPort)") else { return }
        opener.open(url)
    }

    /// Wire up the transport once the socket is connected.
    func attach(sender: any RequestSending) {
        self.sender = sender
    }

    // MARK: Derived state

    /// Number of ports currently in the `.forwarding` state (menu-bar badge).
    public var activeForwardCount: Int {
        state.ports.filter {
            if case .forwarding = $0.forward { return true }
            return false
        }.count
    }

    var isConnected: Bool { state.status == .connected }

    /// The local port a remote port is currently forwarded to, if forwarding.
    func localPort(forRemote remotePort: UInt16) -> UInt16? {
        for entry in state.ports where entry.remotePort.value == remotePort {
            if case .forwarding(let local) = entry.forward { return local.value }
        }
        return nil
    }

    func isPortIntentPending(remotePort: UInt16) -> Bool {
        pendingPortIntents[remotePort] != nil
    }

    // MARK: Applying daemon messages

    /// Apply a decoded daemon message to the published state (main-actor isolated).
    func apply(_ message: DaemonMessage) {
        switch message {
        case .state(let snapshot):
            let previous = state
            state = snapshot
            resolvePendingPortIntents(with: snapshot)
            rememberConnectedHost(snapshot)
            openNewlyForwardedPorts(previous: previous, current: snapshot)
        case .ack(let id, let error, let hostList):
            if let error {
                if let remotePort = pendingRequestPorts.removeValue(forKey: id) {
                    pendingPortIntents.removeValue(forKey: remotePort)
                }
                showToast(error.userMessage, isError: true)
            } else {
                pendingRequestPorts.removeValue(forKey: id)
            }
            if let hostList {
                hosts = hostList
            }
        case .event(let event):
            apply(event)
        }
    }

    /// When `openBrowserOnForward` is set, open the browser for each remote port
    /// that has just TRANSITIONED into forwarding — i.e. it was not forwarding in
    /// the prior snapshot (idle/error/absent) but is forwarding now. Ports that
    /// were already forwarding are skipped, so routine refresh/state pushes never
    /// trigger spurious re-opens.
    private func openNewlyForwardedPorts(previous: PortsState, current: PortsState) {
        guard prefs.openBrowserOnForward else { return }

        var previousLocalPort: [UInt16: UInt16] = [:]
        for entry in previous.ports {
            if case .forwarding(let local) = entry.forward {
                previousLocalPort[entry.remotePort.value] = local.value
            }
        }

        for entry in current.ports {
            guard case .forwarding(let local) = entry.forward else { continue }
            // Already forwarding before (to any local port): not a new transition.
            if previousLocalPort[entry.remotePort.value] != nil { continue }
            openLocalPort(local.value)
        }
    }

    /// Persist the host the daemon reports while connected, so the last
    /// connection is restored on the next launch regardless of how it was
    /// chosen (Settings picker or daemon default).
    private func rememberConnectedHost(_ snapshot: PortsState) {
        guard snapshot.status == .connected,
              let host = snapshot.host,
              !host.isEmpty,
              host != prefs.host
        else { return }
        prefs.host = host
        prefs.save(to: defaults)
    }

    private func apply(_ event: DaemonEvent) {
        switch event {
        case .fileTransfer(let ok, let detail):
            showToast(detail, isError: !ok)
        }
    }

    // MARK: Toasts

    func showToast(_ message: String, isError: Bool) {
        toast = Toast(message: message, isError: isError)
    }

    func dismissToast() {
        toast = nil
    }

    // MARK: Intents

    /// Persist the chosen host, push it as config, and connect.
    func setHost(_ alias: String) async {
        prefs.host = alias
        prefs.save(to: defaults)
        await pushConfig()
        await send(.connect)
    }

    func forward(remotePort: UInt16, localPort: UInt16? = nil) async {
        guard beginPortIntent(.start, remotePort: remotePort) else { return }
        await send(.startForward(
            remotePort: Port(remotePort),
            localPort: localPort.map(Port.init)
        ), pendingRemotePort: remotePort)
    }

    func activate(remotePort: UInt16) async {
        guard localPort(forRemote: remotePort) == nil else { return }
        await forward(remotePort: remotePort)
    }

    func stop(remotePort: UInt16) async {
        guard beginPortIntent(.stop, remotePort: remotePort) else { return }
        await send(.stopForward(remotePort: Port(remotePort)), pendingRemotePort: remotePort)
    }

    /// Re-forward a remote port pinned to a specific local port.
    func setLocalPort(remotePort: UInt16, localPort: UInt16) async {
        guard beginPortIntent(.start, remotePort: remotePort) else { return }
        await send(
            .startForward(remotePort: Port(remotePort), localPort: Port(localPort)),
            pendingRemotePort: remotePort
        )
    }

    func sendFile(localPath: String, remotePath: String? = nil) async {
        await send(.sendFile(localPath: localPath, remotePath: remotePath))
    }

    func refresh() async {
        await send(.refresh)
    }

    func listHosts() async {
        await send(.listHosts)
    }

    func pushConfig() async {
        await send(.setConfig(
            hostAlias: prefs.host,
            refreshSecs: prefs.autoRefreshSecs,
            autoReconnect: prefs.autoReconnect
        ))
    }

    func disconnect() async {
        await send(.disconnect)
    }

    func shutdown() async {
        await send(.shutdown)
    }

    // MARK: Sending

    private func send(_ body: RequestBody, pendingRemotePort: UInt16? = nil) async {
        guard let sender else {
            if let pendingRemotePort {
                pendingPortIntents.removeValue(forKey: pendingRemotePort)
            }
            return
        }
        let id = nextRequestId
        nextRequestId &+= 1
        if let pendingRemotePort {
            pendingRequestPorts[id] = pendingRemotePort
        }
        do {
            try await sender.send(Request(id: id, body: body))
        } catch {
            if let pendingRemotePort {
                pendingRequestPorts.removeValue(forKey: id)
                pendingPortIntents.removeValue(forKey: pendingRemotePort)
            }
            showToast("Request failed: \(error)", isError: true)
        }
    }

    private func beginPortIntent(_ intent: PendingPortIntent, remotePort: UInt16) -> Bool {
        guard pendingPortIntents[remotePort] == nil else { return false }
        pendingPortIntents[remotePort] = intent
        return true
    }

    private func resolvePendingPortIntents(with snapshot: PortsState) {
        let statesByRemotePort = Dictionary(
            uniqueKeysWithValues: snapshot.ports.map { ($0.remotePort.value, $0.forward) }
        )

        for (remotePort, intent) in pendingPortIntents {
            guard let forward = statesByRemotePort[remotePort] else {
                pendingPortIntents.removeValue(forKey: remotePort)
                continue
            }

            switch (intent, forward) {
            case (.start, .forwarding), (.start, .error), (.stop, .idle), (.stop, .error):
                pendingPortIntents.removeValue(forKey: remotePort)
            default:
                break
            }
        }
    }
}

// MARK: - ProtocolError user message

extension ProtocolError {
    /// Short human-readable description for toasts.
    var userMessage: String {
        switch self {
        case .notConnected: return "Not connected"
        case .connectFailed(let detail): return "Connect failed: \(detail)"
        case .bindFailed(let port, let detail): return "Bind failed on \(port.value): \(detail)"
        case .unknownHost(let alias): return "Unknown host: \(alias)"
        case .sendFileFailed(let detail): return "Send file failed: \(detail)"
        case .badRequest(let detail): return "Bad request: \(detail)"
        }
    }
}
