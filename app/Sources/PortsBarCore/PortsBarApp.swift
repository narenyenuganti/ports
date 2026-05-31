import SwiftUI

// MARK: - App scene
//
// The SwiftUI App lives in the library so the logic is testable; the thin
// PortsBar executable target (main.swift) calls PortsBarApp.main().

public struct PortsBarApp: App {
    @StateObject private var model = AppModel()
    @StateObject private var coordinator: AppCoordinator

    public init() {
        let model = AppModel()
        _model = StateObject(wrappedValue: model)
        _coordinator = StateObject(wrappedValue: AppCoordinator(model: model))
    }

    public var body: some Scene {
        MenuBarExtra {
            PopoverView()
                .environmentObject(model)
                .environmentObject(coordinator)
        } label: {
            MenuBarLabel(model: model)
        }
        .menuBarExtraStyle(.window)
    }
}

// MARK: - Menu bar label
//
// SF Symbol "arrow.left.arrow.right" with an active-forward count badge.
// Dimmed when disconnected; warning tint on error.

struct MenuBarLabel: View {
    @ObservedObject var model: AppModel

    var body: some View {
        let count = model.activeForwardCount
        Image(systemName: "arrow.left.arrow.right")
            .symbolRenderingMode(.hierarchical)
            .foregroundStyle(tint)
            .opacity(model.state.status == .disconnected ? 0.5 : 1.0)
            .accessibilityLabel("Ports: \(count) active")
        if count > 0 {
            Text("\(count)")
        }
    }

    private var tint: Color {
        switch model.state.status {
        case .connected: return .primary
        case .connecting: return .secondary
        case .disconnected: return .secondary
        case .error: return .orange
        }
    }
}
