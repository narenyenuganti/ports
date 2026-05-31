import SwiftUI

// MARK: - App entry point

@main
struct PortsBarApp: App {
    @StateObject private var model = AppModel()
    @StateObject private var coordinator: AppCoordinator

    init() {
        let model = AppModel()
        _model = StateObject(wrappedValue: model)
        _coordinator = StateObject(wrappedValue: AppCoordinator(model: model))
    }

    var body: some Scene {
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
            .opacity(model.state.connStatus == .disconnected ? 0.5 : 1.0)
            // Badge: rendered as a label suffix; MenuBarExtra shows text labels.
            .accessibilityLabel("Ports: \(count) active")
        if count > 0 {
            Text("\(count)")
        }
    }

    private var tint: Color {
        switch model.state.connStatus {
        case .connected: return .primary
        case .connecting: return .secondary
        case .disconnected: return .secondary
        case .error: return .orange
        }
    }
}
