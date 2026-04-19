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

                GroupBox("Binary and Database") {
                    Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                        FieldRow(title: "Binary", value: appModel.client.resolvedBinaryPath() ?? "Not found", showPlaceholder: true)
                        FieldRow(title: "Database", value: appModel.serviceStatus?.dbPath, showPlaceholder: true)
                        FieldRow(title: "Service method", value: appModel.serviceStatus?.preferredProvider, showPlaceholder: true)
                        FieldRow(title: "Latest Capture", value: appModel.serviceStatus?.recentCaptureAt, showPlaceholder: true)
                        FieldRow(title: "Retention", value: appModel.serviceStatus?.retention, showPlaceholder: true)
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
    }
}
