import Foundation
import SwiftUI

// MARK: - Request sending abstraction
//
// Abstracts the daemon transport so AppModel is testable without a real socket.
// DaemonClient conforms via an extension below.

protocol RequestSending: Sendable {
    func send(_ request: Request) async throws
}

extension DaemonClient: RequestSending {}

// MARK: - Toast
//
// Transient user-facing message (Ack errors, file-transfer results, etc.).

struct Toast: Identifiable, Equatable, Sendable {
    let id = UUID()
    var message: String
    var isError: Bool
}

// MARK: - Preferences
//
// Persisted to UserDefaults; the daemon-relevant subset is pushed via SetConfig.

struct Preferences: Equatable, Sendable {
    var host: String = ""
    var autoReconnect: Bool = true
    var autoRefreshSecs: UInt64 = 0
    var openBrowserOnForward: Bool = false
    var launchAtLogin: Bool = false

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
final class AppModel: ObservableObject {
    /// Latest daemon state. Defaults to disconnected/empty.
    @Published var state = StateSnapshot(host: nil, status: .disconnected, statusDetail: nil, ports: [])
    /// Persisted preferences.
    @Published var prefs: Preferences
    /// Available SSH hosts (from ListHosts Ack).
    @Published var hosts: [String] = []
    /// Active toast, if any.
    @Published var toast: Toast?

    private let defaults: UserDefaults
    private var sender: (any RequestSending)?
    private var nextRequestId: UInt64 = 1

    init(defaults: UserDefaults = .standard, sender: (any RequestSending)? = nil) {
        self.defaults = defaults
        self.sender = sender
        self.prefs = Preferences.load(from: defaults)
    }

    /// Wire up the transport once the socket is connected.
    func attach(sender: any RequestSending) {
        self.sender = sender
    }

    // MARK: Derived state

    /// Number of ports currently in the `.forwarding` state (menu-bar badge).
    var activeForwardCount: Int {
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

    // MARK: Applying daemon messages

    /// Apply a decoded daemon message to the published state (main-actor isolated).
    func apply(_ message: DaemonMessage) {
        switch message {
        case .state(let snapshot):
            state = snapshot
        case .ack(_, let error, let hostList):
            if let error {
                showToast(error.userMessage, isError: true)
            }
            if let hostList {
                hosts = hostList
            }
        case .event(let event):
            apply(event)
        }
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
        await send(.startForward(
            remotePort: Port(remotePort),
            localPort: localPort.map(Port.init)
        ))
    }

    func stop(remotePort: UInt16) async {
        await send(.stopForward(remotePort: Port(remotePort)))
    }

    /// Re-forward a remote port pinned to a specific local port.
    func setLocalPort(remotePort: UInt16, localPort: UInt16) async {
        await send(.startForward(remotePort: Port(remotePort), localPort: Port(localPort)))
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

    private func send(_ body: RequestBody) async {
        guard let sender else { return }
        let id = nextRequestId
        nextRequestId &+= 1
        do {
            try await sender.send(Request(id: id, body: body))
        } catch {
            showToast("Request failed: \(error)", isError: true)
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
