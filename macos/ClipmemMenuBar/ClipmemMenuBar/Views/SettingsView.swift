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
    @State private var confirmRetention = false

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
                Toggle("Enable Option-Shift-V global hotkey", isOn: $hotkeyEnabled)
                if let message = appModel.hotkeyMessage {
                    Text(message)
                        .foregroundStyle(.orange)
                }
                Toggle("Open Clipmem at login", isOn: launchAtLoginBinding)
                if let message = appModel.launchAtLoginError?.message {
                    Text(message)
                        .foregroundStyle(.orange)
                } else if let message = appModel.launchAtLoginStatus.message {
                    Text(message)
                        .foregroundStyle(.secondary)
                }
                updateSettingsSection
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
                        confirmRetention = true
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
            .confirmationDialog("Apply retention policy?", isPresented: $confirmRetention) {
                Button("Apply") {
                    Task {
                        await appModel.runAction(.settingsRetention(retentionValue), successMessage: "Retention updated")
                        await appModel.refreshSettings()
                    }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Items older than this threshold may be purged during the next cleanup cycle.")
            }
            .tabItem {
                Label("Capture", systemImage: "hand.raised")
            }

            VStack(alignment: .leading, spacing: Spacing.md) {
                HStack {
                    TextField("App identifier (e.g., com.apple.Safari)", text: $newIgnoredBundleID)
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

            VStack(alignment: .leading, spacing: Spacing.lg) {
                GroupBox {
                    Label("Your clipboard archive stays on this Mac.", systemImage: "checkmark.shield")
                }
                GroupBox("Storage") {
                    VStack(alignment: .leading, spacing: Spacing.sm) {
                        Text("The database defaults to ~/Library/Application Support/clipmem/. See Diagnostics for the exact path.")
                        Text("The database is not encrypted. Enable FileVault for at-rest protection.")
                            .foregroundStyle(.secondary)
                    }
                }
                GroupBox("What Gets Captured") {
                    VStack(alignment: .leading, spacing: Spacing.sm) {
                        Text("Images and PDFs are stored as-is. Text content is not processed by AI.")
                        Text("Search is keyword-based, not AI or cloud-powered.")
                        Text("The \"Copied while in\" label is a best guess based on the active app.")
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer()
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
            .tabItem {
                Label("Privacy", systemImage: "lock")
            }
        }
        .overlay(alignment: .top) {
            ActionFeedbackOverlay(message: appModel.actionMessage)
                .padding(.top, Spacing.sm)
        }
    }

    private var pauseBinding: Binding<Bool> {
        Binding {
            appModel.settingsReport?.paused ?? false
        } set: { value in
            Task {
                await appModel.runAction(.settingsPause(value), successMessage: value ? "Capture paused" : "Capture resumed")
                await appModel.refreshSettings()
            }
        }
    }

    private var apiKeyFilterBinding: Binding<Bool> {
        Binding {
            appModel.settingsReport?.apiKeyFilterEnabled ?? false
        } set: { value in
            Task {
                await appModel.runAction(.settingsAPIKeyFilter(value), successMessage: value ? "API-key filter enabled" : "API-key filter disabled")
                await appModel.refreshSettings()
            }
        }
    }

    private var launchAtLoginBinding: Binding<Bool> {
        Binding {
            appModel.launchAtLoginEnabled
        } set: { value in
            appModel.setLaunchAtLoginEnabled(value)
        }
    }

    @ViewBuilder
    private var updateSettingsSection: some View {
        Section("Updates") {
            LabeledContent("Current version", value: appModel.updateStatus.currentVersion)
            LabeledContent("Latest checked version", value: appModel.updateStatus.latestVersion ?? "Not checked")
            LabeledContent("Last checked", value: lastUpdateCheckDescription)

            HStack {
                Button("Check for Updates", systemImage: "arrow.clockwise") {
                    Task { await appModel.checkForUpdates() }
                }
                .disabled(appModel.updateStatus.isChecking)
                if appModel.updateStatus.isChecking {
                    ProgressView()
                        .controlSize(.small)
                }
            }

            if appModel.updateStatus.isUpdateAvailable {
                if appModel.updateStatus.shouldShowHomebrewCommand {
                    VStack(alignment: .leading, spacing: Spacing.sm) {
                        Text("Update with Homebrew")
                            .font(.headline)
                        Text(UpdateChecker.homebrewUpgradeCommand)
                            .font(.caption.monospaced())
                            .textSelection(.enabled)
                        Button("Copy Upgrade Command", systemImage: "doc.on.doc") {
                            appModel.copyUpgradeCommand()
                        }
                    }
                } else {
                    VStack(alignment: .leading, spacing: Spacing.sm) {
                        Text("Download from GitHub Releases")
                            .font(.headline)
                        Button("Open Release", systemImage: "arrow.up.right.square") {
                            appModel.openUpdateRelease()
                        }
                        .disabled(appModel.updateStatus.releaseURL == nil)
                    }
                }
            }

            if let message = appModel.updateStatus.errorMessage {
                Text(message)
                    .foregroundStyle(.orange)
            }
        }
    }

    private var lastUpdateCheckDescription: String {
        guard let lastCheckedAt = appModel.updateStatus.lastCheckedAt else {
            return "Never"
        }
        return lastCheckedAt.formatted(date: .abbreviated, time: .shortened)
    }

    private func addIgnoredBundleID() {
        let value = newIgnoredBundleID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard value.isEmpty == false else { return }
        Task {
            await appModel.runAction(.settingsIgnoreAdd(value), successMessage: "App ignored")
            newIgnoredBundleID = ""
            await appModel.refreshSettings()
        }
    }
}
