import AppKit
import SwiftUI

struct MenuBarPanelView: View {
    let appModel: AppModel

    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings
    @State private var recentSearchQuery = ""
    @State private var restoringItemID: Int?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            healthBanner
                .padding([.horizontal, .top])

            if appModel.updateStatus.isUpdateAvailable {
                UpdateBanner(
                    status: appModel.updateStatus,
                    onCopyCommand: { appModel.copyUpgradeCommand() },
                    onOpenRelease: { appModel.openUpdateRelease() }
                )
                .padding([.horizontal, .top])
            }

            recentsSearchField
                .padding([.horizontal, .top])
                .padding(.bottom, Spacing.sm)

            recentsContent

            Divider()

            footer
                .padding(Spacing.md)
        }
        .animation(.easeInOut(duration: 0.25), value: appModel.healthState)
        .animation(.easeInOut(duration: 0.25), value: appModel.updateStatus.isUpdateAvailable)
        .onAppear {
            Task {
                await appModel.refreshRecentPreviewIfStale(maxAge: 1)
            }
        }
    }

    // MARK: - Health Banner

    @ViewBuilder
    private var healthBanner: some View {
        let state = appModel.healthState
        HealthBanner(
            state: state,
            errorDetail: appModel.lastError,
            isRunningAction: appModel.isRunningAction,
            actionLabel: healthActionLabel(for: state),
            onAction: { healthAction(for: state) }
        )
    }

    private func healthActionLabel(for state: HealthState) -> String? {
        switch state {
        case .setupNeeded: "Run Setup"
        case .missingBinary: "Open Settings"
        case .watcherStopped: "Start"
        case .conflict, .error: "Diagnostics"
        case .capturePaused: "Resume"
        case .stale, .noRecentCaptures: "Refresh"
        case .healthy, .unknown: nil
        }
    }

    private func healthAction(for state: HealthState) {
        switch state {
        case .setupNeeded:
            Task { await appModel.runSetup() }
        case .missingBinary:
            WindowActivation.openSettings(openSettings)
        case .watcherStopped:
            Task { await appModel.serviceAction("start") }
        case .conflict, .error:
            appModel.requestSettingsTab(.diagnostics)
            WindowActivation.openSettings(openSettings)
        case .capturePaused:
            Task { await appModel.runAction(.settingsPause(false), successMessage: "Capture resumed") }
        case .stale, .noRecentCaptures:
            Task { await appModel.refreshAll() }
        case .healthy, .unknown:
            break
        }
    }

    // MARK: - Search

    private var recentsSearchField: some View {
        TextField("Filter recent clips\u{2026}", text: $recentSearchQuery)
            .textFieldStyle(.roundedBorder)
            .controlSize(.small)
    }

    // MARK: - Clipboard Items

    @ViewBuilder
    private var recentsContent: some View {
        if appModel.recentPreview.isEmpty && !appModel.isRefreshing {
            EmptyStateView(
                title: "Start copying",
                detail: "Items appear here automatically.",
                symbol: "clipboard"
            )
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
            List {
                ForEach(filteredRecentPreview) { item in
                    Button {
                        restoringItemID = item.snapshotId
                        Task {
                            await appModel.restore(item)
                            try? await Task.sleep(for: .milliseconds(200))
                            restoringItemID = nil
                            NSApp.deactivate()
                        }
                    } label: {
                        ResultRowView(item: item, selected: item.snapshotId == restoringItemID)
                    }
                    .buttonStyle(.plain)
                    .contextMenu {
                        Button("Copy Plain Text") {
                            if let text = item.copyablePlainText {
                                PasteboardActions.copyPlainText(text)
                            }
                        }
                        .disabled(item.copyablePlainText == nil)
                        Button("Open in History") {
                            appModel.requestHistoryFocus(
                                snapshotID: item.snapshotId,
                                mode: .recent,
                                query: ""
                            )
                            WindowActivation.openWindow(openWindow, id: .history)
                        }
                        Button("Forget", role: .destructive) {
                            Task { await appModel.forget(item) }
                        }
                    }
                }
            }
            .listStyle(.inset)
        }
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: Spacing.md) {
            Button {
                WindowActivation.openWindow(openWindow, id: .history)
            } label: {
                Label("History", systemImage: "clock.arrow.circlepath")
            }
            .help("Open History (\u{2318}\u{21E7}H)")

            Button {
                WindowActivation.openWindow(openWindow, id: .quickRecall)
            } label: {
                Label("Search", systemImage: "magnifyingglass")
            }
            .help("Open Search (\u{2325}\u{21E7}V)")

            Spacer()

            Button {
                WindowActivation.openSettings(openSettings)
            } label: {
                Label("Settings", systemImage: "gearshape")
                    .labelStyle(.iconOnly)
            }
            .help("Open Settings")

            Button {
                NSApp.terminate(nil)
            } label: {
                Label("Quit", systemImage: "power")
                    .labelStyle(.iconOnly)
            }
            .help("Quit Clipmem")
        }
        .buttonStyle(.borderless)
    }

    // MARK: - Filtering

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
