import SwiftUI

// Placeholder popover; full implementation lands in Phase 4.1.
struct PopoverView: View {
    @EnvironmentObject var model: AppModel
    @EnvironmentObject var coordinator: AppCoordinator

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Ports")
                .font(.headline)
            Text(model.state.connStatus == .connected ? "Connected" : "Disconnected")
                .foregroundStyle(.secondary)
            Button("Quit") { NSApplication.shared.terminate(nil) }
        }
        .padding()
        .frame(width: 320)
        .task {
            await coordinator.start()
        }
    }
}
