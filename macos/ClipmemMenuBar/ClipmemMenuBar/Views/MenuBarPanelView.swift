import AppKit
import SwiftUI

struct MenuBarPanelView: View {
    let appModel: AppModel

    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings
    @State private var confirmUninstall = false

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.lg) {
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

            serviceSummary
            quickActions

            Divider()

            HStack {
                Text("Recent")
                    .font(.headline)
                Spacer()
                Button("Open History", systemImage: "clock.arrow.circlepath") {
                    WindowActivation.openWindow(openWindow, id: .history)
                }
            }

            if appModel.recentPreview.isEmpty && !appModel.isRefreshing {
                EmptyStateView(title: "No recent copies", detail: "Clipboard entries will appear here as you copy.", symbol: "clipboard")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(appModel.recentPreview) { item in
                    ResultRowView(item: item, selected: false)
                        .contextMenu {
                            Button("Restore") {
                                Task { await appModel.restore(item) }
                            }
                            Button("Copy Plain Text") {
                                PasteboardActions.copyPlainText(item.bestText ?? item.previewText ?? "")
                            }
                        }
                }
                .listStyle(.inset)
            }

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
    }

    private var serviceSummary: some View {
        Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.xs) {
            FieldRow(title: "Provider", value: appModel.serviceStatus?.preferredProvider)
            FieldRow(title: "Capture", value: appModel.serviceStatus?.recentCaptureAt ?? "No recent capture")
            FieldRow(title: "Database", value: appModel.serviceStatus?.dbExists == true ? "Available" : "Missing")
            FieldRow(title: "Policy", value: policySummary)
        }
        .font(.callout)
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
                Button("Uninstall Service") {
                    confirmUninstall = true
                }
                Button("Run Doctor") {
                    Task { await appModel.refreshDoctor() }
                }
                Button("Open Logs Folder") {
                    appModel.openLogsFolder()
                }
                .disabled(appModel.serviceStatus?.logPaths.isEmpty != false)
            }
            .disabled(appModel.isRunningAction)
            .confirmationDialog("Uninstall the clipmem background service?", isPresented: $confirmUninstall) {
                Button("Uninstall", role: .destructive) {
                    Task { await appModel.serviceAction("uninstall") }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This stops clipboard capture. Your saved history is kept. You can reinstall with Setup.")
            }
            if appModel.isRunningAction {
                ProgressView()
                    .controlSize(.small)
            }
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }

    private var policySummary: String {
        let paused = appModel.serviceStatus?.paused == true ? "paused" : "active"
        let filter = appModel.serviceStatus?.apiKeyFilterEnabled == true ? "API-key filter on" : "API-key filter off"
        return "\(paused), \(filter)"
    }
}
