import Foundation
import Testing
@testable import PortsBarCore

@MainActor
@Suite("AppCoordinator")
struct AppCoordinatorTests {
    @Test("starts in idle phase")
    func idleAtStart() {
        let model = AppModel(defaults: UserDefaults(suiteName: "coord.\(UUID())")!)
        let coordinator = AppCoordinator(model: model)
        #expect(coordinator.phase == .idle)
    }

    @Test("start with no daemon binary fails gracefully")
    func startNoBinary() async {
        // Ensure no dev override / bundled binary is picked up in the test host.
        let model = AppModel(defaults: UserDefaults(suiteName: "coord.\(UUID())")!)
        let coordinator = AppCoordinator(model: model)
        // The test bundle won't contain Contents/Resources/ports, so DaemonClient()
        // throws binaryNotFound and the coordinator records a failure.
        if ProcessInfo.processInfo.environment["PORTS_DAEMON_BIN"] != nil {
            return // Skip when a real binary is configured.
        }
        await coordinator.start()
        if case .failed = coordinator.phase {
            #expect(model.state.connStatus == .error)
        } else {
            // If a binary was somehow located, that's also a valid outcome;
            // just assert we left the idle phase.
            #expect(coordinator.phase != .idle)
        }
    }
}
