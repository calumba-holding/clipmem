import AppKit
import SwiftUI

struct MenuBarPanelView: View {
    let model: AppModel

    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                StatusBadge(state: model.healthState)
                Spacer()
                Button("Refresh", systemImage: "arrow.clockwise") {
                    Task { await model.refreshAll() }
                }
                .labelStyle(.iconOnly)
                .buttonStyle(.borderless)
                .help("Refresh status")
            }

            if let message = model.lastErrorMessage {
                ErrorBanner(message: message)
            }
            if let message = model.actionMessage {
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

            List(model.recentPreview) { item in
                ResultRowView(item: item, selected: false)
                    .contextMenu {
                        Button("Restore") {
                            Task { await model.restore(item) }
                        }
                        Button("Copy Plain Text") {
                            PasteboardActions.copyPlainText(item.bestText ?? item.previewText ?? "")
                        }
                    }
            }
            .listStyle(.inset)

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
        Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 4) {
            FieldRow(title: "Provider", value: model.serviceStatus?.preferredProvider)
            FieldRow(title: "Capture", value: model.serviceStatus?.recentCaptureAt ?? "No recent capture")
            FieldRow(title: "Database", value: model.serviceStatus?.dbExists == true ? "Available" : "Missing")
            FieldRow(title: "Policy", value: policySummary)
        }
        .font(.callout)
    }

    private var quickActions: some View {
        HStack {
            Button("Setup", systemImage: "wrench.and.screwdriver") {
                Task { await model.runSetup() }
            }
            .disabled(model.isRunningAction)
            Button("Start", systemImage: "play.fill") {
                Task { await model.serviceAction("start") }
            }
            .disabled(model.isRunningAction)
            Button("Stop", systemImage: "stop.fill") {
                Task { await model.serviceAction("stop") }
            }
            .disabled(model.isRunningAction)
            Menu("More", systemImage: "ellipsis.circle") {
                Button("Uninstall Service") {
                    Task { await model.serviceAction("uninstall") }
                }
                Button("Run Doctor") {
                    Task { await model.refreshDoctor() }
                }
                Button("Open Logs Folder") {
                    model.openLogsFolder()
                }
                .disabled(model.serviceStatus?.logPaths.isEmpty != false)
            }
            .disabled(model.isRunningAction)
            if model.isRunningAction {
                ProgressView()
                    .controlSize(.small)
            }
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
    }

    private var policySummary: String {
        let paused = model.serviceStatus?.paused == true ? "paused" : "active"
        let filter = model.serviceStatus?.apiKeyFilterEnabled == true ? "API-key filter on" : "API-key filter off"
        return "\(paused), \(filter)"
    }
}
