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
    private let hosting: NSHostingController<AnyView>
    private let makeRoot: @MainActor () -> AnyView
    private var cancellables: Set<AnyCancellable> = []
    private var keyMonitor: Any?

    init(model: AppModel, coordinator: AppCoordinator, openSettings: @escaping @MainActor () -> Void) {
        self.model = model
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        popover = NSPopover()
        popover.behavior = .transient
        popover.animates = false
        let make: @MainActor () -> AnyView = {
            AnyView(
                PopoverView()
                    .environmentObject(model)
                    .environmentObject(coordinator)
                    .environment(\.openSettingsAction, openSettings)
            )
        }
        self.makeRoot = make
        self.hosting = NSHostingController(rootView: make())
        popover.contentViewController = hosting
        installKeyboardMonitor()

        if let button = statusItem.button {
            button.target = self
            button.action = #selector(togglePopover(_:))
        }

        // Re-render the menu-bar icon whenever daemon state changes, and force
        // the popover's hosted SwiftUI view to flush its pending update.
        //
        // A detached NSHostingController inside an NSPopover schedules SwiftUI
        // invalidations from async (daemon-stream) model changes but does not
        // commit them until an AppKit event triggers a layout pass — so the
        // tiles would otherwise lag one click behind the live model. We nudge a
        // layout pass on the next run-loop turn (after @Published commits the
        // new value) to flush the update immediately.
        model.$state
            .sink { [weak self] state in
                MainActor.assumeIsolated {
                    guard let self else { return }
                    self.updateButton(for: state)
                    self.schedulePopoverContentFlush()
                }
            }
            .store(in: &cancellables)
        updateButton(for: model.state)
    }

    /// Force the popover's hosted SwiftUI view to process any pending update by
    /// rebuilding its root view (SwiftUI's own observation does not re-render a
    /// detached NSHostingController for async model changes).
    private func flushPopoverContent() {
        guard popover.isShown else { return }
        hosting.rootView = makeRoot()
    }

    private func schedulePopoverContentFlush() {
        Task { @MainActor [weak self] in
            await Task.yield()
            self?.flushPopoverContent()
        }
    }

    @objc private func togglePopover(_ sender: Any?) {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(sender)
        } else {
            model.ensureSelection()
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            // Bring the popover's window forward so it can take key (accessory app).
            popover.contentViewController?.view.window?.makeKey()
        }
    }

    private func installKeyboardMonitor() {
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            let handled = MainActor.assumeIsolated {
                self.handlePopoverKeyDown(event)
            }
            return handled ? nil : event
        }
    }

    private func handlePopoverKeyDown(_ event: NSEvent) -> Bool {
        guard popover.isShown else { return false }
        guard !isTextInputActive else { return false }
        guard event.modifierFlags.intersection([.command, .option, .control]).isEmpty else {
            return false
        }
        guard let command = PopoverKeyboardCommand(event: event) else { return false }

        switch command {
        case .selectPrevious:
            model.selectPreviousPort()
        case .selectNext:
            model.selectNextPort()
        case .toggleSelected:
            if model.isPortFiltering {
                model.confirmPortFilter()
            } else {
                Task { await model.toggleSelectedPort() }
            }
        case .beginFilter:
            if model.isPortFiltering {
                model.appendPortFilter("/")
            } else {
                model.beginPortFilter()
            }
        case .appendFilter(let text):
            model.appendPortFilter(text)
        case .deleteFilterCharacter:
            model.deletePortFilterCharacter()
        case .cancelFilter:
            model.cancelPortFilter()
        }

        return true
    }

    private var isTextInputActive: Bool {
        guard let responder = popover.contentViewController?.view.window?.firstResponder else {
            return false
        }
        return responder is NSTextView || responder is NSTextField
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
