import AppKit
import SwiftUI

// MARK: - PortTileView
//
// One tile per remote PortEntry: compact identity, status pill, and on-tap
// actions (Forward/Stop, Open, Copy URL, custom local port). Thin view;
// intents go through AppModel.

struct PortTilePresentation {
    let primaryLabel: String?
    let primaryValue: String
    let detail: String?
    let portAccessory: String?

    init(entry: PortEntry) {
        let remotePort = entry.remotePort.value
        if let process = entry.process, !process.isEmpty {
            primaryLabel = nil
            primaryValue = process
            portAccessory = ":\(remotePort)"
        } else {
            primaryLabel = "port"
            primaryValue = "\(remotePort)"
            portAccessory = nil
        }

        if case .forwarding(let localPort) = entry.forward {
            detail = "localhost:\(localPort.value)"
        } else {
            detail = nil
        }
    }
}

struct PortTileView: View {
    @EnvironmentObject var model: AppModel
    let entry: PortEntry

    @State private var expanded = false
    @State private var customLocalPort = ""

    private var remotePort: UInt16 { entry.remotePort.value }

    private var localPort: UInt16? {
        if case .forwarding(let p) = entry.forward { return p.value }
        return nil
    }

    private var isForwarding: Bool { localPort != nil }

    private var isPending: Bool {
        model.isPortIntentPending(remotePort: remotePort)
    }

    private var presentation: PortTilePresentation {
        PortTilePresentation(entry: entry)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                handleHeaderTap()
            } label: {
                tileHeader
            }
            .buttonStyle(.plain)

            if expanded {
                actions
            }
        }
        .padding(.horizontal, PopoverLayout.portTileHorizontalPadding)
        .padding(.vertical, PopoverLayout.portTileVerticalPadding)
        .background(
            .quaternary.opacity(0.4),
            in: RoundedRectangle(cornerRadius: PopoverLayout.portTileCornerRadius)
        )
        .onChange(of: localPort) { _, newValue in
            if let newValue {
                customLocalPort = "\(newValue)"
            }
        }
    }

    private var tileHeader: some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 1) {
                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    if let label = presentation.primaryLabel {
                        Text(label)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                    Text(presentation.primaryValue)
                        .font(.callout.weight(.semibold))
                        .lineLimit(1)
                    if let accessory = presentation.portAccessory {
                        portAccessory(accessory)
                    }
                }
                if let detail = presentation.detail {
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
            statusPill
        }
        .contentShape(Rectangle())
    }

    @ViewBuilder
    private var statusPill: some View {
        if isPending {
            pill(isForwarding ? "Updating" : "Forwarding", isForwarding ? .yellow : .green)
        } else {
            switch entry.forward {
            case .forwarding:
                pill("Forwarding", .green)
            case .idle:
                pill("Idle", .secondary)
            case .error(let detail):
                pill("Error", .red)
                    .help(detail)
            }
        }
    }

    private func pill(_ text: String, _ color: Color) -> some View {
        Text(text)
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(color.opacity(0.18), in: Capsule())
            .foregroundStyle(color == .secondary ? Color.secondary : color)
    }

    private func portAccessory(_ text: String) -> some View {
        Text(text)
            .font(.caption2.weight(.semibold))
            .monospacedDigit()
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(.secondary.opacity(0.14), in: Capsule())
            .foregroundStyle(.secondary)
    }

    @ViewBuilder
    private var actions: some View {
        HStack(spacing: 8) {
            Text("localhost")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            TextField(localPort.map(String.init) ?? "auto", text: $customLocalPort)
                .textFieldStyle(.roundedBorder)
                .monospacedDigit()
                .frame(width: 90)
            Button("Apply") {
                if let p = UInt16(customLocalPort) {
                    Task { await model.setLocalPort(remotePort: remotePort, localPort: p) }
                }
            }
            .controlSize(.small)
            .disabled(isPending || UInt16(customLocalPort) == nil || UInt16(customLocalPort) == localPort)
            Spacer()
        }
        .disabled(isPending)

        HStack(spacing: 8) {
            if isForwarding {
                Button("Stop") {
                    Task { await model.stop(remotePort: remotePort) }
                }
                .disabled(isPending)
                Button {
                    openInBrowser()
                } label: {
                    Label("Open", systemImage: "arrow.up.right.square")
                }
                Button {
                    copyURL()
                } label: {
                    Label("Copy URL", systemImage: "doc.on.doc")
                }
            }
            Spacer()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }

    // App-side actions (no daemon round-trip), using the bound local port.

    private func openInBrowser() {
        guard let local = localPort else { return }
        model.openLocalPort(local)
    }

    private func copyURL() {
        guard let local = localPort else { return }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString("http://localhost:\(local)", forType: .string)
    }

    private func handleHeaderTap() {
        if let localPort {
            customLocalPort = "\(localPort)"
            expanded.toggle()
            return
        }

        expanded = true
        Task { await model.activate(remotePort: remotePort) }
    }
}
