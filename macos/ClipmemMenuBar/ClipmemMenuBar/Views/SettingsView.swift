import SwiftUI

struct ClipmemSettingsView: View {
    let appModel: AppModel

    @AppStorage(PreferenceKey.binaryPathOverride) private var binaryPathOverride = ""
    @AppStorage(PreferenceKey.databasePathOverride) private var databasePathOverride = ""
    @AppStorage(PreferenceKey.defaultRecentHours) private var defaultRecentHours = 24
    @AppStorage(PreferenceKey.defaultQueryMode) private var defaultQueryMode = QueryMode.recent.rawValue
    @AppStorage(PreferenceKey.hotkeyEnabled) private var hotkeyEnabled = true
    @State private var selectedTab: SettingsTab = .general
    @State private var handledSettingsOpenRequestID = 0
    @State private var newIgnoredBundleID = ""
    @State private var retentionValue = "forever"
    @State private var confirmRetention = false
    @State private var confirmCompact = false
    @State private var confirmCompressImages = false
    @State private var showManualPurge = false
    @State private var confirmUninstall = false

    var body: some View {
        TabView(selection: $selectedTab) {
            generalTab
                .tag(SettingsTab.general)
                .tabItem { Label(SettingsTab.general.title, systemImage: SettingsTab.general.symbol) }

            storageTab
                .tag(SettingsTab.storage)
                .tabItem { Label(SettingsTab.storage.title, systemImage: SettingsTab.storage.symbol) }

            captureTab
                .tag(SettingsTab.capture)
                .tabItem { Label(SettingsTab.capture.title, systemImage: SettingsTab.capture.symbol) }

            ignoredAppsTab
                .tag(SettingsTab.ignoredApps)
                .tabItem { Label(SettingsTab.ignoredApps.title, systemImage: SettingsTab.ignoredApps.symbol) }

            diagnosticsTab
                .tag(SettingsTab.diagnostics)
                .tabItem { Label(SettingsTab.diagnostics.title, systemImage: SettingsTab.diagnostics.symbol) }

            privacyTab
                .tag(SettingsTab.privacy)
                .tabItem { Label(SettingsTab.privacy.title, systemImage: SettingsTab.privacy.symbol) }
        }
        .overlay(alignment: .top) {
            ActionFeedbackOverlay(message: appModel.actionMessage)
                .padding(.top, Spacing.sm)
        }
        .task {
            applyPendingSettingsOpenRequestIfNeeded()
            await refreshSettingsSurface()
        }
        .onChange(of: appModel.pendingSettingsOpenRequest?.id) {
            applyPendingSettingsOpenRequestIfNeeded()
        }
    }

    private var generalTab: some View {
        Form {
            TextField("clipmem binary", text: $binaryPathOverride)
            TextField("Database path", text: $databasePathOverride)
            Stepper("Recent window: \(defaultRecentHours) hours", value: $defaultRecentHours, in: 1...720)
            Picker("Default mode", selection: defaultDisplayModeBinding) {
                ForEach(DisplayMode.allCases) { mode in
                    Text(mode.title).tag(mode)
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

            serviceSection
            updateSettingsSection
        }
        .formStyle(.grouped)
        .padding()
        .confirmationDialog("Uninstall the clipmem service?", isPresented: $confirmUninstall) {
            Button("Uninstall Service", role: .destructive) {
                Task { await appModel.serviceAction("uninstall") }
            }
            Button("Keep Service", role: .cancel) {}
        } message: {
            Text("This removes the LaunchAgent or Homebrew service registration. Your clipboard database is preserved.")
        }
    }

    private var storageTab: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Spacing.lg) {
                GroupBox("Archive Storage") {
                    VStack(alignment: .leading, spacing: Spacing.md) {
                        Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                            FieldRow(title: "Database size", value: databaseSizeDescription, showPlaceholder: true)
                            FieldRow(title: "Database path", value: databasePathDescription, showPlaceholder: true)
                            FieldRow(title: "Retention", value: appModel.settingsReport?.retention, showPlaceholder: true)
                        }

                        Text("Copied screenshots and image-heavy clips can take significant disk space. Compression keeps the archive searchable while reducing eligible stored image bytes.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }

                StorageActionRow(
                    title: "Compress Images",
                    detail: "Convert eligible stored screenshots and images to lossless WebP when it saves space, then compact the database.",
                    systemImage: "photo.stack",
                    buttonTitle: "Compress Images",
                    disabled: appModel.isRunningAction
                ) {
                    confirmCompressImages = true
                }

                StorageActionRow(
                    title: "Compact Database",
                    detail: "Return unused SQLite and WAL pages to disk without changing clipboard history.",
                    systemImage: "archivebox",
                    buttonTitle: "Compact Database",
                    disabled: appModel.isRunningAction
                ) {
                    confirmCompact = true
                }

                StorageActionRow(
                    title: "Purge Old History",
                    detail: "Preview matching snapshots before permanently deleting old clipboard history.",
                    systemImage: "trash",
                    buttonTitle: "Purge Old History...",
                    role: .destructive,
                    disabled: appModel.isRunningAction
                ) {
                    showManualPurge = true
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .confirmationDialog("Compress stored images?", isPresented: $confirmCompressImages) {
            Button("Compress Images") {
                Task { await appModel.optimizeImages() }
            }
            Button("Keep Images As Is", role: .cancel) {}
        } message: {
            Text("Clipmem converts eligible screenshots and images to lossless WebP only when it saves space. Image content stays visually identical, already processed images are skipped, and the database is compacted afterward.")
        }
        .confirmationDialog("Compact database?", isPresented: $confirmCompact) {
            Button("Compact Database") {
                Task { await appModel.compactDatabase() }
            }
            Button("Leave Database As Is", role: .cancel) {}
        } message: {
            Text("This reclaims unused SQLite and WAL disk space without deleting clipboard history. The operation may need temporary disk space while SQLite rebuilds the database.")
        }
        .sheet(isPresented: $showManualPurge) {
            ManualPurgeSheet(appModel: appModel, initialDuration: retentionValue)
        }
    }

    private var captureTab: some View {
        Form {
            Toggle("Pause capture", isOn: pauseBinding)
            Toggle("API-key filter", isOn: apiKeyFilterBinding)
            Toggle("OCR for copied images", isOn: ocrBinding)
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
        .confirmationDialog("Apply retention policy?", isPresented: $confirmRetention) {
            Button("Apply Retention") {
                Task {
                    await appModel.runAction(.settingsRetention(retentionValue), successMessage: "Retention updated")
                    await appModel.refreshSettings()
                }
            }
            Button("Keep Current Retention", role: .cancel) {}
        } message: {
            Text("Items older than this threshold may be purged during the next cleanup cycle.")
        }
    }

    private var ignoredAppsTab: some View {
        VStack(alignment: .leading, spacing: Spacing.md) {
            HStack {
                TextField("App identifier (for example, com.apple.Safari)", text: $newIgnoredBundleID)
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
                        .help("Remove \(bundleID)")
                    }
                }
            }
            Text("The menu bar app adds io.openclaw.clipmem.menubar by default to avoid self-capture noise.")
                .foregroundStyle(.secondary)
        }
        .padding()
    }

    private var diagnosticsTab: some View {
        DiagnosticsView(appModel: appModel)
    }

    private var privacyTab: some View {
        VStack(alignment: .leading, spacing: Spacing.lg) {
            GroupBox {
                Label("Your clipboard archive stays on this Mac.", systemImage: "checkmark.shield")
            }
            GroupBox("Storage") {
                VStack(alignment: .leading, spacing: Spacing.sm) {
                    Text("The database defaults to ~/Library/Application Support/clipmem/. See Settings > Diagnostics for the exact path.")
                    Text("The database is not encrypted. Enable FileVault for at-rest protection.")
                        .foregroundStyle(.secondary)
                }
            }
            GroupBox("What Gets Captured") {
                VStack(alignment: .leading, spacing: Spacing.sm) {
                    Text("Images and PDFs are stored as-is unless you use Settings > Storage to compress eligible images.")
                    Text("Text content is not processed by AI.")
                    Text("Search is keyword-based, not AI or cloud-powered.")
                    Text("The \"Copied while in\" label is a best guess based on the active app.")
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var serviceSection: some View {
        Section("Service") {
            LabeledContent("Status", value: appModel.healthState.title)
            VStack(alignment: .leading, spacing: Spacing.sm) {
                SettingsActionButton("Setup", systemImage: "wrench.and.screwdriver") {
                    Task { await appModel.runSetup() }
                }
                SettingsActionButton("Start", systemImage: "play.fill") {
                    Task { await appModel.serviceAction("start") }
                }
                SettingsActionButton("Stop", systemImage: "stop.fill") {
                    Task { await appModel.serviceAction("stop") }
                }
            }
            .disabled(appModel.isRunningAction)

            Button("Uninstall Service", role: .destructive) {
                confirmUninstall = true
            }
            .disabled(appModel.isRunningAction)
        }
    }

    private var defaultDisplayModeBinding: Binding<DisplayMode> {
        Binding {
            let mode = QueryMode(rawValue: defaultQueryMode) ?? .recent
            return DisplayMode.from(queryMode: mode).displayMode
        } set: { newDisplayMode in
            switch newDisplayMode {
            case .search: defaultQueryMode = QueryMode.recall.rawValue
            case .recent: defaultQueryMode = QueryMode.recent.rawValue
            case .timeline: defaultQueryMode = QueryMode.timeline.rawValue
            }
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

    private var ocrBinding: Binding<Bool> {
        Binding {
            appModel.settingsReport?.ocrEnabled ?? false
        } set: { value in
            Task {
                await appModel.runAction(.settingsOCR(value), successMessage: value ? "OCR enabled" : "OCR disabled")
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

    private var databaseSizeDescription: String? {
        DisplayFormatters.byteCount(appModel.serviceStatus?.dbSizeBytes)
    }

    private var databasePathDescription: String? {
        appModel.serviceStatus?.dbPath ?? databasePathOverride
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

    private func refreshSettingsSurface() async {
        await appModel.refreshSettings()
        await appModel.refreshStatus()
        retentionValue = appModel.settingsReport?.retention ?? "forever"
    }

    private func applyPendingSettingsOpenRequestIfNeeded() {
        guard let request = appModel.pendingSettingsOpenRequest else { return }
        guard request.id != handledSettingsOpenRequestID else { return }
        handledSettingsOpenRequestID = request.id
        selectedTab = request.tab
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

private struct StorageActionRow: View {
    let title: String
    let detail: String
    let systemImage: String
    let buttonTitle: String
    var role: ButtonRole?
    var disabled = false
    let action: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: Spacing.md) {
            Image(systemName: systemImage)
                .font(.title3)
                .foregroundStyle(isDestructive ? .red : .blue)
                .frame(width: 28)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: Spacing.xs) {
                Text(title)
                    .font(.headline)
                Text(detail)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: Spacing.lg)

            Button(buttonTitle, role: role, action: action)
                .buttonStyle(.borderedProminent)
                .controlSize(.regular)
                .fixedSize(horizontal: true, vertical: false)
                .disabled(disabled)
        }
        .padding(Spacing.lg)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: .rect(cornerRadius: Spacing.sm))
        .accessibilityElement(children: .combine)
    }

    private var isDestructive: Bool {
        role != nil
    }
}

private struct SettingsActionButton: View {
    let title: String
    let systemImage: String
    let action: () -> Void

    init(_ title: String, systemImage: String, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            Label(title, systemImage: systemImage)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.bordered)
        .controlSize(.regular)
    }
}
