import AppKit
import SwiftUI

enum PopoverLayout {
    static let width: CGFloat = 420
    static let portListMaxHeight: CGFloat = 920
    static let portListSpacing: CGFloat = 5
    static let portTileHorizontalPadding: CGFloat = 10
    static let portTileVerticalPadding: CGFloat = 7
    static let portTileCornerRadius: CGFloat = 7
}

// MARK: - PopoverView
//
// Thin model-view: header (status dot + host + gear), one tile per PortEntry,
// footer (Refresh / Send file... / Settings / Quit). All logic lives in
// AppModel; views only read state and fire intents.

public struct PopoverView: View {
    @EnvironmentObject var model: AppModel
    @EnvironmentObject var coordinator: AppCoordinator

    public init() {}

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HeaderView()
                .padding(.horizontal, 12)
                .padding(.top, 10)
                .padding(.bottom, 8)

            Divider()

            if let toast = model.toast {
                ToastView(toast: toast) { model.dismissToast() }
                    .padding(.horizontal, 12)
                    .padding(.top, 8)
            }

            content
                .padding(.horizontal, 12)
                .padding(.vertical, 8)

            Divider()

            FooterView()
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
        }
        .frame(width: PopoverLayout.width)
    }

    @ViewBuilder
    private var content: some View {
        if case .failed(let reason) = coordinator.phase {
            VStack(alignment: .leading, spacing: 8) {
                Text(reason)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Button("Retry") { Task { await coordinator.retry() } }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        } else if model.state.ports.isEmpty {
            Text(model.isConnected ? "No listening ports found." : "Not connected.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 12)
        } else {
            ScrollView {
                VStack(spacing: PopoverLayout.portListSpacing) {
                    ForEach(model.state.ports, id: \.remotePort) { entry in
                        PortTileView(entry: entry)
                    }
                }
            }
            .frame(maxHeight: PopoverLayout.portListMaxHeight)
        }
    }
}

// MARK: - Settings action environment

/// Injected by StatusItemController so the gear button can open the AppKit
/// settings window. Defaults to a no-op (e.g. in previews/tests).
private struct OpenSettingsActionKey: EnvironmentKey {
    static let defaultValue: @MainActor () -> Void = {}
}

extension EnvironmentValues {
    var openSettingsAction: @MainActor () -> Void {
        get { self[OpenSettingsActionKey.self] }
        set { self[OpenSettingsActionKey.self] = newValue }
    }
}

// MARK: - Header

struct HeaderView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.openSettingsAction) private var openSettings

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(statusColor)
                .frame(width: 9, height: 9)
            Text(model.state.host ?? "No host")
                .font(.headline)
            if let detail = model.state.statusDetail {
                Text(detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Button {
                openSettings()
            } label: {
                Image(systemName: "gearshape")
            }
            .buttonStyle(.borderless)
            .help("Settings")
        }
    }

    private var statusColor: Color {
        switch model.state.status {
        case .connected: return .green
        case .connecting: return .yellow
        case .disconnected: return .gray
        case .error: return .red
        }
    }
}

// MARK: - Footer

struct FooterView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        HStack(spacing: 12) {
            Button { Task { await model.refresh() } } label: {
                Label("Refresh", systemImage: "arrow.clockwise")
            }
            FileTransferButton()
            Spacer()
            Button(role: .destructive) {
                Task {
                    await model.shutdown()
                    NSApplication.shared.terminate(nil)
                }
            } label: {
                Label("Quit", systemImage: "power")
            }
        }
        .buttonStyle(.borderless)
        .font(.callout)
    }
}

// MARK: - Toast

struct ToastView: View {
    let toast: Toast
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: toast.isError ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                .foregroundStyle(toast.isError ? .red : .green)
            Text(toast.message)
                .font(.caption)
                .lineLimit(2)
            Spacer()
            Button { onDismiss() } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.borderless)
        }
        .padding(8)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
    }
}
