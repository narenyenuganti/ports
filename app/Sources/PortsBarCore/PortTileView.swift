import AppKit
import SwiftUI

// MARK: - PortTileView
//
// One tile per remote PortEntry: process name, "remote :p -> localhost:p",
// a status pill, and on-tap actions (Forward/Stop, Open, Copy URL, custom
// local port). Thin view; intents go through AppModel.

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

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                expanded.toggle()
            } label: {
                tileHeader
            }
            .buttonStyle(.plain)

            if expanded {
                actions
            }
        }
        .padding(8)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
    }

    private var tileHeader: some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(entry.process ?? "port \(remotePort)")
                    .font(.callout.weight(.medium))
                Text(routeDescription)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            statusPill
        }
        .contentShape(Rectangle())
    }

    private var routeDescription: String {
        if let local = localPort {
            return "remote :\(remotePort) -> localhost:\(local)"
        }
        return "remote :\(remotePort)"
    }

    @ViewBuilder
    private var statusPill: some View {
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

    private func pill(_ text: String, _ color: Color) -> some View {
        Text(text)
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(color.opacity(0.18), in: Capsule())
            .foregroundStyle(color == .secondary ? Color.secondary : color)
    }

    @ViewBuilder
    private var actions: some View {
        HStack(spacing: 8) {
            if isForwarding {
                Button("Stop") { Task { await model.stop(remotePort: remotePort) } }
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
            } else {
                Button("Forward") { Task { await model.forward(remotePort: remotePort) } }
            }
            Spacer()
        }
        .buttonStyle(.bordered)
        .controlSize(.small)

        HStack(spacing: 6) {
            TextField("local port", text: $customLocalPort)
                .textFieldStyle(.roundedBorder)
                .frame(width: 90)
            Button("Set") {
                if let p = UInt16(customLocalPort) {
                    Task { await model.setLocalPort(remotePort: remotePort, localPort: p) }
                    customLocalPort = ""
                }
            }
            .controlSize(.small)
            .disabled(UInt16(customLocalPort) == nil)
        }
    }

    // App-side actions (no daemon round-trip), using the bound local port.

    private func openInBrowser() {
        guard let local = localPort,
              let url = URL(string: "http://localhost:\(local)") else { return }
        NSWorkspace.shared.open(url)
    }

    private func copyURL() {
        guard let local = localPort else { return }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString("http://localhost:\(local)", forType: .string)
    }
}
