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
// Transient user-facing message (Ack errors, file-transfer completion, etc.).

struct Toast: Identifiable, Equatable, Sendable {
    let id = UUID()
    var message: String
    var isError: Bool
}

// MARK: - Preferences
//
// Mirrors the daemon-relevant config plus app-only toggles. Persisted to
// UserDefaults; pushed to the daemon as DaemonConfig via SetConfig.

struct Preferences: Equatable, Sendable {
    var autoReconnect: Bool = true
    var autoRefreshSecs: UInt64 = 0
    var openBrowserOnForward: Bool = false
    var launchAtLogin: Bool = false
    var lastHost: String = ""

    var daemonConfig: DaemonConfig {
        DaemonConfig(
            autoReconnect: autoReconnect,
            autoRefreshSecs: autoRefreshSecs,
            openBrowserOnForward: openBrowserOnForward
        )
    }

    private enum Keys {
        static let autoReconnect = "autoReconnect"
        static let autoRefreshSecs = "autoRefreshSecs"
        static let openBrowserOnForward = "openBrowserOnForward"
        static let launchAtLogin = "launchAtLogin"
        static let lastHost = "lastHost"
    }

    static func load(from defaults: UserDefaults) -> Preferences {
        var p = Preferences()
        if defaults.object(forKey: Keys.autoReconnect) != nil {
            p.autoReconnect = defaults.bool(forKey: Keys.autoReconnect)
        }
        p.autoRefreshSecs = UInt64(max(0, defaults.integer(forKey: Keys.autoRefreshSecs)))
        p.openBrowserOnForward = defaults.bool(forKey: Keys.openBrowserOnForward)
        p.launchAtLogin = defaults.bool(forKey: Keys.launchAtLogin)
        p.lastHost = defaults.string(forKey: Keys.lastHost) ?? ""
        return p
    }

    func save(to defaults: UserDefaults) {
        defaults.set(autoReconnect, forKey: Keys.autoReconnect)
        defaults.set(Int(autoRefreshSecs), forKey: Keys.autoRefreshSecs)
        defaults.set(openBrowserOnForward, forKey: Keys.openBrowserOnForward)
        defaults.set(launchAtLogin, forKey: Keys.launchAtLogin)
        defaults.set(lastHost, forKey: Keys.lastHost)
    }
}

// MARK: - AppModel

@MainActor
final class AppModel: ObservableObject {
    /// Latest daemon state. Defaults to disconnected/empty.
    @Published var state = StateSnapshot(host: nil, connStatus: .disconnected, forwards: [])
    /// Persisted preferences.
    @Published var prefs: Preferences
    /// Available SSH hosts (from ListHosts Ack).
    @Published var hosts: [HostAlias] = []
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

    /// Number of forwards currently in the `.forwarding` state.
    var activeForwardCount: Int {
        state.forwards.filter {
            if case .forwarding = $0.status { return true }
            return false
        }.count
    }

    var isConnected: Bool { state.connStatus == .connected }

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
        case .connStatusChanged(let connStatus):
            state.connStatus = connStatus
        case .forwardAdded(let forward):
            if let idx = state.forwards.firstIndex(where: { $0.id == forward.id }) {
                state.forwards[idx] = forward
            } else {
                state.forwards.append(forward)
            }
        case .forwardRemoved(let forwardId):
            state.forwards.removeAll { $0.id == forwardId }
        case .fileTransfer(let localPath, _, _, let done):
            if done {
                let name = (localPath as NSString).lastPathComponent
                showToast("Sent \(name)", isError: false)
            }
        case .log(let level, let message):
            if level == "error" {
                showToast(message, isError: true)
            }
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

    func setHost(_ alias: String) async {
        prefs.lastHost = alias
        prefs.save(to: defaults)
        await send(.connect(host: HostAlias(alias)))
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

    /// Convenience: re-forward an existing entry at a specific local port.
    func setLocalPort(remotePort: UInt16, localPort: UInt16) async {
        await send(.startForward(remotePort: Port(remotePort), localPort: Port(localPort)))
    }

    func sendFile(localPath: String, remotePath: String? = nil) async {
        await send(.sendFile(localPath: localPath, remotePath: remotePath))
    }

    func refresh() async {
        await send(.listHosts)
    }

    func pushConfig() async {
        await send(.setConfig(config: prefs.daemonConfig))
    }

    func disconnect() async {
        await send(.disconnect)
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
        case .hostNotFound(let alias): return "Host not found: \(alias)"
        case .connectionFailed(let reason): return "Connection failed: \(reason)"
        case .ssh(let message): return "SSH error: \(message)"
        case .bindFailed(let port, let reason): return "Bind failed on \(port): \(reason)"
        case .forwardNotFound(let id): return "Forward not found: \(id)"
        case .notConnected: return "Not connected"
        case .invalidRequest(let reason): return "Invalid request: \(reason)"
        case .io(let message): return "I/O error: \(message)"
        case .timeout: return "Timed out"
        case .internal(let message): return "Internal error: \(message)"
        }
    }
}
