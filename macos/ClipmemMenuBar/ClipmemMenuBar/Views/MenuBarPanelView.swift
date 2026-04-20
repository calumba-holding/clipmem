import AppKit
import SwiftUI

struct MenuBarPanelView: View {
    let appModel: AppModel

    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings
    @State private var recentSearchQuery = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.md) {
            HStack {
                StatusBadge(state: appModel.healthState)
                Spacer()
                Button("Refresh", systemImage: "arrow.clockwise") {
                    Task { await appModel.refreshAll() }
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help("Refresh status")
            }

            if let error = appModel.lastError {
                ErrorBanner(message: error.message, recovery: error.recovery)
            }
            if let message = appModel.actionMessage {
                Label(message, systemImage: "checkmark.circle")
                    .font(.callout)
                    .foregroundStyle(.green)
                    .lineLimit(2)
            }
            updateBanner

            serviceSummary
            quickActions

            Divider()

            recentsHeader
            recentsSearchField
            recentsContent

            Divider()

            HStack {
                Button("Quick Recall", systemImage: "bolt") {
                    WindowActivation.openWindow(openWindow, id: .quickRecall)
                }
                Button {
                    WindowActivation.openSettings(openSettings)
                } label: {
                    Label("Settings", systemImage: "gearshape")
                }
                Spacer()
                Button("Quit", systemImage: "power") {
                    NSApp.terminate(nil)
                }
            }
        }
        .padding()
        .onAppear {
            Task {
                await appModel.refreshRecentPreviewIfStale(maxAge: 1)
            }
        }
    }

    private var serviceSummary: some View {
        Grid(alignment: .leading, horizontalSpacing: Spacing.lg, verticalSpacing: Spacing.sm) {
            GridRow {
                CompactStatusMetric(title: "Watcher", value: watcherSummary)
                CompactStatusMetric(title: "Latest", value: latestCaptureSummary)
            }
            GridRow {
                CompactStatusMetric(title: "Database", value: databaseSummary)
                CompactStatusMetric(title: "Policy", value: policySummary)
            }
        }
        .font(.caption)
    }

    private var quickActions: some View {
        HStack {
            Button("Setup", systemImage: "wrench.and.screwdriver") {
                Task { await appModel.runSetup() }
            }
            .disabled(appModel.isRunningAction)
            Button("Start", systemImage: "play.fill") {
                Task { await appModel.serviceAction("start") }
            }
            .disabled(appModel.isRunningAction)
            Button("Stop", systemImage: "stop.fill") {
                Task { await appModel.serviceAction("stop") }
            }
            .disabled(appModel.isRunningAction)
            Menu("More", systemImage: "ellipsis.circle") {
                Button("Compact Database") {
                    confirmMenuAction(.compactDatabase)
                }
                Button("Optimize Images...") {
                    confirmMenuAction(.optimizeImages)
                }
                Divider()
                Button("Uninstall Service") {
                    confirmMenuAction(.uninstallService)
                }
                Button("Run Doctor") {
                    Task { await appModel.refreshDoctor() }
                }
                Button("Check for Updates") {
                    Task { await appModel.checkForUpdates() }
                }
                .disabled(appModel.updateStatus.isChecking)
                Button("Open Logs Folder") {
                    appModel.openLogsFolder()
                }
                .disabled(appModel.serviceStatus?.logPaths.isEmpty != false)
            }
            .disabled(appModel.isRunningAction)
            if appModel.isRunningAction {
                ProgressView()
                    .controlSize(.small)
            }
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }

    private func confirmMenuAction(_ confirmation: MenuBarConfirmation) {
        Task { @MainActor in
            await Task.yield()
            guard ConfirmationAlertPresenter.confirm(confirmation) else { return }

            switch confirmation {
            case .compactDatabase:
                await appModel.compactDatabase()
            case .optimizeImages:
                await appModel.optimizeImages()
            case .uninstallService:
                await appModel.serviceAction("uninstall")
            }
        }
    }

    @ViewBuilder
    private var updateBanner: some View {
        if appModel.updateStatus.isUpdateAvailable {
            VStack(alignment: .leading, spacing: Spacing.sm) {
                HStack(alignment: .firstTextBaseline) {
                    Label("Update Available", systemImage: "arrow.down.circle.fill")
                        .font(.headline)
                    Spacer()
                    Text(appModel.updateStatus.latestVersion ?? "")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Text("Clipmem \(appModel.updateStatus.latestVersion ?? "the latest release") is available. You have \(appModel.updateStatus.currentVersion).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    if appModel.updateStatus.shouldShowHomebrewCommand {
                        Button("Copy Command", systemImage: "doc.on.doc") {
                            appModel.copyUpgradeCommand()
                        }
                    }
                    Button("Open Release", systemImage: "arrow.up.right.square") {
                        appModel.openUpdateRelease()
                    }
                    .disabled(appModel.updateStatus.releaseURL == nil)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .padding(Spacing.md)
            .background(.blue.opacity(0.08), in: RoundedRectangle(cornerRadius: Spacing.sm))
        }
    }

    private var policySummary: String {
        let paused = appModel.serviceStatus?.paused == true ? "paused" : "active"
        let filter = appModel.serviceStatus?.apiKeyFilterEnabled == true ? "API-key filter on" : "API-key filter off"
        return "\(paused), \(filter)"
    }

    private var latestCaptureSummary: String {
        DisplayFormatters.localTimestamp(appModel.serviceStatus?.recentCaptureAt) ?? "No captures yet"
    }

    private var databaseSummary: String {
        guard appModel.serviceStatus?.dbExists == true else { return "Missing" }
        return DisplayFormatters.byteCount(appModel.serviceStatus?.dbSizeBytes) ?? "Size unavailable"
    }

    private var watcherSummary: String {
        guard let status = appModel.serviceStatus else { return "Unknown" }
        if status.conflict == true { return "Multiple watchers running" }
        if status.launchagent?.running == true { return "LaunchAgent running" }
        if status.homebrew?.running == true { return "Homebrew running" }
        if status.launchagent?.installed == true || status.launchagent?.loaded == true {
            return "LaunchAgent stopped"
        }
        if status.homebrew?.installed == true || status.homebrew?.loaded == true {
            return "Homebrew stopped"
        }
        return "Not set up"
    }

    private var recentsHeader: some View {
        HStack {
            Text("Recent")
                .font(.headline)
            Spacer()
            Button("Open History", systemImage: "clock.arrow.circlepath") {
                WindowActivation.openWindow(openWindow, id: .history)
            }
        }
    }

    private var recentsSearchField: some View {
        TextField("Search recents", text: $recentSearchQuery)
            .textFieldStyle(.roundedBorder)
            .controlSize(.small)
    }

    @ViewBuilder
    private var recentsContent: some View {
        if appModel.recentPreview.isEmpty && !appModel.isRefreshing {
            EmptyStateView(title: "No recent copies", detail: "Clipboard entries will appear here as you copy.", symbol: "clipboard")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if filteredRecentPreview.isEmpty && recentSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false {
            VStack(spacing: Spacing.md) {
                EmptyStateView(
                    title: "No matching recents",
                    detail: "Search the full archive in History.",
                    symbol: "magnifyingglass"
                )
                Button("Open History Search", systemImage: "arrow.up.right.square") {
                    appModel.requestHistorySearch(query: recentSearchQuery)
                    WindowActivation.openWindow(openWindow, id: .history)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            List(filteredRecentPreview) { item in
                ResultRowView(item: item, selected: false)
                    .contextMenu {
                        Button("Restore") {
                            Task { await appModel.restore(item) }
                        }
                        Button("Copy Plain Text") {
                            if let text = item.copyablePlainText {
                                PasteboardActions.copyPlainText(text)
                            }
                        }
                        .disabled(item.copyablePlainText == nil)
                    }
            }
            .listStyle(.inset)
        }
    }

    private var filteredRecentPreview: [ClipmemItem] {
        let query = recentSearchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard query.isEmpty == false else { return appModel.recentPreview }
        return appModel.recentPreview.filter { item in
            searchableText(for: item).localizedCaseInsensitiveContains(query)
        }
    }

    private func searchableText(for item: ClipmemItem) -> String {
        [
            item.displayText,
            item.appName,
            item.appBundleId,
            item.kind,
            DisplayFormatters.localTimestamp(item.observedAt),
            item.observedAt,
            item.urls?.joined(separator: " "),
            item.filePaths?.joined(separator: " "),
        ]
        .compactMap { $0 }
        .joined(separator: " ")
    }
}

private struct CompactStatusMetric: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.caption.weight(.medium))
                .lineLimit(1)
                .truncationMode(.middle)
                .help(value)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
