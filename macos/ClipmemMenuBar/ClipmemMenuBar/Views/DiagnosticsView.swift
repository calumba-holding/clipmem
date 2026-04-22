import SwiftUI

struct DiagnosticsView: View {
    let appModel: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Spacing.lg) {
                StatusBadge(state: appModel.healthState)

                if let error = appModel.lastError {
                    ErrorBanner(message: error.message, recovery: error.recovery)
                }

                GroupBox("Service") {
                    VStack(alignment: .leading, spacing: Spacing.sm) {
                        DiagnosticsActionButton("Setup", systemImage: "wrench.and.screwdriver") {
                            Task { await appModel.runSetup() }
                        }
                        DiagnosticsActionButton("Start", systemImage: "play.fill") {
                            Task { await appModel.serviceAction("start") }
                        }
                        DiagnosticsActionButton("Stop", systemImage: "stop.fill") {
                            Task { await appModel.serviceAction("stop") }
                        }
                    }
                    .disabled(appModel.isRunningAction)
                }

                GroupBox("Binary and Database") {
                    VStack(alignment: .leading, spacing: Spacing.md) {
                        Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                            FieldRow(title: "Binary", value: appModel.client.resolvedBinaryPath() ?? "Not found", showPlaceholder: true)
                            FieldRow(title: "Watcher binary", value: appModel.serviceStatus?.watcherBinaryPath, showPlaceholder: true)
                            FieldRow(title: "Database", value: appModel.serviceStatus?.dbPath, showPlaceholder: true)
                            FieldRow(title: "Service method", value: appModel.serviceStatus?.preferredProvider, showPlaceholder: true)
                            FieldRow(title: "Latest Capture", value: appModel.serviceStatus?.recentCaptureAt, showPlaceholder: true)
                            FieldRow(title: "Retention", value: appModel.serviceStatus?.retention, showPlaceholder: true)
                        }
                        if appModel.serviceStatus?.watcherBinaryMismatch == true,
                           let note = appModel.serviceStatus?.watcherBinaryMismatchNote {
                            Label(note, systemImage: "exclamationmark.triangle.fill")
                                .foregroundStyle(.orange)
                                .font(.callout)
                                .textSelection(.enabled)
                        }
                        DiagnosticsActionButton("Open Logs Folder", systemImage: "folder") {
                            appModel.openLogsFolder()
                        }
                        .disabled(appModel.serviceStatus?.logPaths.isEmpty != false)
                    }
                }

                GroupBox("Doctor") {
                    Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                        FieldRow(title: "SQLite", value: appModel.doctorReport?.sqliteVersion, showPlaceholder: true)
                        FieldRow(title: "Database mode", value: appModel.doctorReport?.journalMode, showPlaceholder: true)
                        FieldRow(title: "Full-text search", value: appModel.doctorReport?.fts5CreateVirtualTableOk.map { $0 ? "Available" : "Not available" }, showPlaceholder: true)
                    }
                    Button("Run Doctor", systemImage: "stethoscope") {
                        Task { await appModel.refreshDoctor() }
                    }
                    .padding(.top, Spacing.sm)
                }

                if let notes = appModel.serviceStatus?.notes, notes.isEmpty == false {
                    GroupBox("Notes") {
                        VStack(alignment: .leading, spacing: Spacing.sm) {
                            ForEach(notes, id: \.self) { note in
                                Text(note)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .task {
            await appModel.refreshDoctor()
        }
    }
}

private struct DiagnosticsActionButton: View {
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
