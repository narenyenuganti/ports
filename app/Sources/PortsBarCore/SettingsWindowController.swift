import AppKit
import SwiftUI

// MARK: - SettingsWindowController
//
// A real, standalone settings window (titled, closable with the red X and
// Cmd-W) hosting the SwiftUI SettingsView. Replaces the popover-attached
// .sheet so settings open as their own window, like CodexBar. The window is
// not released on close so reopening preserves position and state.

@MainActor
final class SettingsWindowController: NSWindowController {
    init(model: AppModel) {
        let hosting = NSHostingController(
            rootView: SettingsView().environmentObject(model)
        )
        let window = NSWindow(contentViewController: hosting)
        window.title = "Ports Settings"
        window.styleMask = [.titled, .closable, .miniaturizable]
        window.isReleasedWhenClosed = false
        window.center()
        super.init(window: window)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }
}
