import Foundation
import SwiftUI

// MARK: - Connection phase

public enum ConnectionPhase: Equatable, Sendable {
    case idle
    case starting
    case connected
    case failed(String)
}

// MARK: - AppCoordinator
//
// Owns the DaemonClient lifecycle and pumps its message stream into AppModel.
// @MainActor because it mutates the model and publishes connection phase to UI.

@MainActor
public final class AppCoordinator: ObservableObject {
    @Published public private(set) var phase: ConnectionPhase = .idle

    private let model: AppModel
    private var client: DaemonClient?
    private var pumpTask: Task<Void, Never>?

    public init(model: AppModel) {
        self.model = model
    }

    /// Ensure the daemon is running, connect, attach the transport, and start
    /// consuming the message stream. Safe to call on retry.
    public func start() async {
        guard phase != .starting, phase != .connected else { return }
        phase = .starting
        do {
            let client = try DaemonClient()
            self.client = client
            let stream = try await client.start()
            model.attach(sender: client)
            phase = .connected
            pump(stream)
            // Push current config; if a host was previously selected, connect to it.
            await model.pushConfig()
            if !model.prefs.host.isEmpty {
                await model.setHost(model.prefs.host)
            }
            await model.listHosts()
        } catch {
            phase = .failed("\(error)")
            model.state.status = .error
        }
    }

    /// Retry after a failure or EOF: tear down and start fresh.
    public func retry() async {
        await teardown()
        await start()
    }

    /// Clean shutdown: disconnect, stop the stream, terminate the daemon.
    public func teardown() async {
        pumpTask?.cancel()
        pumpTask = nil
        if let client {
            await model.disconnect()
            await client.stop()
        }
        client = nil
        phase = .idle
    }

    private func pump(_ stream: AsyncStream<DaemonMessage>) {
        pumpTask = Task { [weak self] in
            for await message in stream {
                self?.model.apply(message)
            }
            // Stream finished: socket EOF. Surface error; UI offers Retry.
            self?.phase = .failed("Daemon connection closed")
            self?.model.state.status = .error
        }
    }
}
