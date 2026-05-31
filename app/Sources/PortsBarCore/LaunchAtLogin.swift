import Foundation
import ServiceManagement

// MARK: - LaunchAtLogin
//
// Wraps SMAppService.mainApp for the Settings toggle (Phase 4.5). Registering
// adds Ports.app as a login item; unregistering removes it. The current status
// is read back so the toggle reflects reality.

enum LaunchAtLogin {
    /// Whether the app is currently registered as a login item.
    static var isEnabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    /// Register or unregister the main app as a login item.
    /// Throws if the system rejects the (un)registration.
    static func setEnabled(_ enabled: Bool) throws {
        let service = SMAppService.mainApp
        if enabled {
            if service.status != .enabled {
                try service.register()
            }
        } else {
            if service.status == .enabled {
                try service.unregister()
            }
        }
    }
}
