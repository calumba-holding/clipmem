import SwiftUI

struct DiagnosticsView: View {
    let appModel: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                StatusBadge(state: appModel.healthState)

                if let message = appModel.lastErrorMessage {
                    ErrorBanner(message: message)
                }

                GroupBox("Binary and Database") {
                    Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                        FieldRow(title: "Binary", value: appModel.client.resolvedBinaryPath() ?? "Not found")
                        FieldRow(title: "Database", value: appModel.serviceStatus?.dbPath)
                        FieldRow(title: "Preferred Provider", value: appModel.serviceStatus?.preferredProvider)
                        FieldRow(title: "Latest Capture", value: appModel.serviceStatus?.recentCaptureAt)
                        FieldRow(title: "Retention", value: appModel.serviceStatus?.retention)
                    }
                }

                GroupBox("Doctor") {
                    Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                        FieldRow(title: "SQLite", value: appModel.doctorReport?.sqliteVersion)
                        FieldRow(title: "Journal", value: appModel.doctorReport?.journalMode)
                        FieldRow(title: "FTS5", value: appModel.doctorReport?.fts5CreateVirtualTableOk.map(String.init))
                    }
                    Button("Run Doctor", systemImage: "stethoscope") {
                        Task { await appModel.refreshDoctor() }
                    }
                    .padding(.top, 8)
                }

                if let notes = appModel.serviceStatus?.notes, notes.isEmpty == false {
                    GroupBox("Notes") {
                        VStack(alignment: .leading, spacing: 6) {
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
