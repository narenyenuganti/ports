import AppKit
import SwiftUI

// MARK: - FileTransferButton
//
// Footer "Send file..." action: NSOpenPanel to pick a local file, then a
// prompt for the remote directory (default /tmp). Fires model.sendFile; the
// resulting FileTransfer event surfaces as a toast (handled in AppModel).

struct FileTransferButton: View {
    @EnvironmentObject var model: AppModel
    @State private var pendingLocalPath: String?
    @State private var remoteDir = "/tmp"
    @State private var showRemotePrompt = false

    var body: some View {
        Button {
            chooseFile()
        } label: {
            Label("Send file...", systemImage: "paperplane")
        }
        .disabled(!model.isConnected)
        .help(model.isConnected ? "Send a local file to the host" : "Connect first")
        .sheet(isPresented: $showRemotePrompt) {
            remotePrompt
        }
    }

    private var remotePrompt: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Send file")
                .font(.headline)
            if let path = pendingLocalPath {
                Text((path as NSString).lastPathComponent)
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            TextField("Remote directory", text: $remoteDir)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("Cancel") {
                    showRemotePrompt = false
                    pendingLocalPath = nil
                }
                Button("Send") {
                    send()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(pendingLocalPath == nil || remoteDir.isEmpty)
            }
        }
        .padding(16)
        .frame(width: 320)
    }

    private func chooseFile() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            pendingLocalPath = url.path
            showRemotePrompt = true
        }
    }

    private func send() {
        guard let local = pendingLocalPath else { return }
        let dir = remoteDir
        showRemotePrompt = false
        pendingLocalPath = nil
        Task { await model.sendFile(localPath: local, remotePath: dir) }
    }
}
