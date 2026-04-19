import SwiftUI

struct ClipmemSettingsView: View {
    let appModel: AppModel

    @AppStorage(PreferenceKey.binaryPathOverride) private var binaryPathOverride = ""
    @AppStorage(PreferenceKey.databasePathOverride) private var databasePathOverride = ""
    @AppStorage(PreferenceKey.defaultRecentHours) private var defaultRecentHours = 24
    @AppStorage(PreferenceKey.defaultQueryMode) private var defaultQueryMode = QueryMode.recent.rawValue
    @AppStorage(PreferenceKey.hotkeyEnabled) private var hotkeyEnabled = true
    @State private var newIgnoredBundleID = ""
    @State private var retentionValue = "forever"

    var body: some View {
        TabView {
            Form {
                TextField("clipmem binary", text: $binaryPathOverride)
                TextField("Database path", text: $databasePathOverride)
                Stepper("Recent window: \(defaultRecentHours) hours", value: $defaultRecentHours, in: 1...720)
                Picker("Default mode", selection: $defaultQueryMode) {
                    ForEach([QueryMode.recall, .search, .recent, .timeline]) { mode in
                        Text(mode.title).tag(mode.rawValue)
                    }
                }
                Toggle("Enable Command-Shift-V global hotkey", isOn: $hotkeyEnabled)
                if let message = appModel.hotkeyMessage {
                    Text(message)
                        .foregroundStyle(.orange)
                }
            }
            .formStyle(.grouped)
            .padding()
            .tabItem {
                Label("General", systemImage: "gear")
            }

            Form {
                Toggle("Pause capture", isOn: pauseBinding)
                Toggle("API-key filter", isOn: apiKeyFilterBinding)
                HStack {
                    TextField("Retention", text: $retentionValue)
                    Button("Apply") {
                        Task {
                            await appModel.runAction(.settingsRetention(retentionValue))
                            await appModel.refreshSettings()
                        }
                    }
                }
                Text("Use values like 30d, 12h, 15m, or forever.")
                    .foregroundStyle(.secondary)
            }
            .formStyle(.grouped)
            .padding()
            .task {
                retentionValue = appModel.settingsReport?.retention ?? "forever"
            }
            .tabItem {
                Label("Capture", systemImage: "hand.raised")
            }

            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    TextField("Bundle ID", text: $newIgnoredBundleID)
                    Button("Add", systemImage: "plus") {
                        addIgnoredBundleID()
                    }
                    .disabled(newIgnoredBundleID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                List {
                    ForEach(appModel.settingsReport?.ignoredBundleIds ?? [], id: \.self) { bundleID in
                        HStack {
                            Text(bundleID)
                                .textSelection(.enabled)
                            Spacer()
                            Button("Remove", systemImage: "minus.circle") {
                                Task {
                                    await appModel.runAction(.settingsIgnoreRemove(bundleID))
                                    await appModel.refreshSettings()
                                }
                            }
                            .labelStyle(.iconOnly)
                        }
                    }
                }
                Text("The menu bar app adds io.openclaw.clipmem.menubar by default to avoid self-capture noise.")
                    .foregroundStyle(.secondary)
            }
            .padding()
            .task { await appModel.refreshSettings() }
            .tabItem {
                Label("Ignored Apps", systemImage: "app.badge")
            }

            VStack(alignment: .leading, spacing: 14) {
                Label("Archive data stays local.", systemImage: "checkmark.shield")
                Text("The database path is shown in Diagnostics and defaults to ~/Library/Application Support/clipmem/clipmem.sqlite3.")
                Text("The database is not encrypted. Use FileVault or another disk encryption layer for at-rest protection.")
                Text("Images and PDFs are stored as clipboard representations but are not OCR'd.")
                Text("App provenance is a best-effort frontmost-app hint. The UI phrases it as copied while in an app.")
                Text("Search is lexical and rule-based, not semantic AI search.")
                Spacer()
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
            .tabItem {
                Label("Privacy", systemImage: "lock")
            }
        }
    }

    private var pauseBinding: Binding<Bool> {
        Binding {
            appModel.settingsReport?.paused ?? false
        } set: { value in
            Task {
                await appModel.runAction(.settingsPause(value))
                await appModel.refreshSettings()
            }
        }
    }

    private var apiKeyFilterBinding: Binding<Bool> {
        Binding {
            appModel.settingsReport?.apiKeyFilterEnabled ?? false
        } set: { value in
            Task {
                await appModel.runAction(.settingsAPIKeyFilter(value))
                await appModel.refreshSettings()
            }
        }
    }

    private func addIgnoredBundleID() {
        let value = newIgnoredBundleID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard value.isEmpty == false else { return }
        Task {
            await appModel.runAction(.settingsIgnoreAdd(value))
            newIgnoredBundleID = ""
            await appModel.refreshSettings()
        }
    }
}
