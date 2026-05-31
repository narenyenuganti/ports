import AppKit
import SwiftUI

// MARK: - App scene
//
// The app is an accessory (LSUIElement) menu-bar app. Presentation is driven
// by AppKit (NSStatusItem + NSPopover) via AppDelegate rather than SwiftUI's
// MenuBarExtra, which glitches when Pickers/menus open. The SwiftUI App owns
// only the (unused) Settings scene required by the App protocol; the real
// settings window is an AppKit NSWindow managed by the delegate.

public struct PortsBarApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    public init() {}

    public var body: some Scene {
        // Accessory apps show no app menu, so this scene is never presented;
        // it exists only to satisfy the App protocol's scene requirement.
        Settings { EmptyView() }
    }
}

// MARK: - AppDelegate
//
// Owns the shared AppModel/AppCoordinator and the AppKit presentation: the
// status item + popover (StatusItemController) and the settings window.

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()
    private(set) lazy var coordinator = AppCoordinator(model: model)

    private var statusController: StatusItemController?
    private var settingsWindow: SettingsWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusController = StatusItemController(
            model: model,
            coordinator: coordinator,
            openSettings: { [weak self] in self?.showSettings() }
        )
        Task { await coordinator.start() }
    }

    /// Show (or focus) the standalone settings window.
    func showSettings() {
        if settingsWindow == nil {
            settingsWindow = SettingsWindowController(model: model)
        }
        NSApp.activate(ignoringOtherApps: true)
        settingsWindow?.showWindow(nil)
        settingsWindow?.window?.makeKeyAndOrderFront(nil)
    }
}
