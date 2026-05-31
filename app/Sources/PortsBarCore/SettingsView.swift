import SwiftUI

// MARK: - SettingsView
//
// Host picker (populated via ListHosts), launch-at-login, auto-refresh
// interval, open-browser-on-forward, and auto-reconnect. Persists to
// UserDefaults via AppModel.prefs; on change pushes SetConfig (and Connect
// when the host changes).

public struct SettingsView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.dismiss) private var dismiss

    @State private var selectedHost = ""
    @State private var autoReconnect = true
    @State private var openBrowserOnForward = false
    @State private var autoRefreshSecs = 0.0
    @State private var launchAtLogin = false
    @State private var launchError: String?

    public init() {}

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Settings").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding(12)

            Divider()

            Form {
                Section("Host") {
                    Picker("Active host", selection: $selectedHost) {
                        Text("None").tag("")
                        ForEach(model.hosts, id: \.self) { host in
                            Text(host).tag(host)
                        }
                    }
                    .onChange(of: selectedHost) { _, newValue in
                        Task { await model.setHost(newValue) }
                    }
                    if model.hosts.isEmpty {
                        Text("No hosts found in ~/.ssh/config.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                Section("Behavior") {
                    Toggle("Launch at login", isOn: $launchAtLogin)
                        .onChange(of: launchAtLogin) { _, newValue in
                            applyLaunchAtLogin(newValue)
                        }
                    if let launchError {
                        Text(launchError)
                            .font(.caption)
                            .foregroundStyle(.red)
                    }
                    Toggle("Open browser on forward", isOn: $openBrowserOnForward)
                        .onChange(of: openBrowserOnForward) { _, newValue in
                            model.prefs.openBrowserOnForward = newValue
                            persistAndPush()
                        }
                    Toggle("Auto-reconnect", isOn: $autoReconnect)
                        .onChange(of: autoReconnect) { _, newValue in
                            model.prefs.autoReconnect = newValue
                            persistAndPush()
                        }
                    VStack(alignment: .leading) {
                        Text("Auto-refresh: \(Int(autoRefreshSecs))s (0 = off)")
                            .font(.caption)
                        Slider(value: $autoRefreshSecs, in: 0...60, step: 5)
                            .onChange(of: autoRefreshSecs) { _, newValue in
                                model.prefs.autoRefreshSecs = UInt64(newValue)
                                persistAndPush()
                            }
                    }
                }
            }
            .formStyle(.grouped)
        }
        .frame(width: 360, height: 420)
        .task { await model.listHosts() }
        .onAppear(perform: loadFromModel)
    }

    private func loadFromModel() {
        selectedHost = model.prefs.host
        autoReconnect = model.prefs.autoReconnect
        openBrowserOnForward = model.prefs.openBrowserOnForward
        autoRefreshSecs = Double(model.prefs.autoRefreshSecs)
        launchAtLogin = LaunchAtLogin.isEnabled
        model.prefs.launchAtLogin = launchAtLogin
    }

    private func persistAndPush() {
        model.prefs.save(to: .standard)
        Task { await model.pushConfig() }
    }

    private func applyLaunchAtLogin(_ enabled: Bool) {
        do {
            try LaunchAtLogin.setEnabled(enabled)
            launchError = nil
            model.prefs.launchAtLogin = LaunchAtLogin.isEnabled
        } catch {
            launchError = "Could not update login item: \(error.localizedDescription)"
            // Reflect the real status back into the toggle.
            launchAtLogin = LaunchAtLogin.isEnabled
        }
    }
}
