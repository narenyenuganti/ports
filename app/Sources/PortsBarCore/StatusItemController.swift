import AppKit
import Combine
import SwiftUI

// MARK: - StatusItemController
//
// Owns the menu-bar NSStatusItem and the NSPopover that hosts the SwiftUI
// PopoverView. Using AppKit's real popover (behavior = .transient) instead of
// SwiftUI's MenuBarExtra(.window) fixes the dropdown/background glitch: clicks
// inside keep the popover open, Escape or a click outside dismisses it, and
// Pickers render in their own layer without blanking the rest of the view.

@MainActor
final class StatusItemController {
    private let statusItem: NSStatusItem
    private let popover: NSPopover
    private let model: AppModel
    private var cancellables: Set<AnyCancellable> = []

    init(model: AppModel, coordinator: AppCoordinator, openSettings: @escaping @MainActor () -> Void) {
        self.model = model
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        popover = NSPopover()
        popover.behavior = .transient
        popover.animates = false
        let root = PopoverView()
            .environmentObject(model)
            .environmentObject(coordinator)
            .environment(\.openSettingsAction, openSettings)
        popover.contentViewController = NSHostingController(rootView: root)

        if let button = statusItem.button {
            button.target = self
            button.action = #selector(togglePopover(_:))
        }

        // Re-render the menu-bar icon whenever daemon state changes.
        model.$state
            .sink { [weak self] state in
                MainActor.assumeIsolated { self?.updateButton(for: state) }
            }
            .store(in: &cancellables)
        updateButton(for: model.state)
    }

    @objc private func togglePopover(_ sender: Any?) {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(sender)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            // Bring the popover's window forward so it can take key (accessory app).
            popover.contentViewController?.view.window?.makeKey()
        }
    }

    private func updateButton(for state: PortsState) {
        guard let button = statusItem.button else { return }
        let count = state.ports.filter {
            if case .forwarding = $0.forward { return true }
            return false
        }.count

        let image = NSImage(
            systemSymbolName: "arrow.left.arrow.right",
            accessibilityDescription: "Ports: \(count) active"
        )
        image?.isTemplate = true
        button.image = image
        button.imagePosition = .imageLeading
        button.title = count > 0 ? " \(count)" : ""

        switch state.status {
        case .connected:
            button.contentTintColor = nil
            button.alphaValue = 1.0
        case .connecting:
            button.contentTintColor = nil
            button.alphaValue = 0.6
        case .disconnected:
            button.contentTintColor = nil
            button.alphaValue = 0.5
        case .error:
            button.contentTintColor = .systemOrange
            button.alphaValue = 1.0
        }
    }
}
