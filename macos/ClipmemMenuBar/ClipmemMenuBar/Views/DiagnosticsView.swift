import SwiftUI

struct DiagnosticsView: View {
    let appModel: AppModel
    @State private var confirmCompact = false
    @State private var confirmOptimizeImages = false
    @State private var showManualPurge = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Spacing.lg) {
                StatusBadge(state: appModel.healthState)

                if let error = appModel.lastError {
                    ErrorBanner(message: error.message, recovery: error.recovery)
                }

                GroupBox("Binary and Database") {
                    VStack(alignment: .leading, spacing: Spacing.md) {
                        Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                            FieldRow(title: "Binary", value: appModel.client.resolvedBinaryPath() ?? "Not found", showPlaceholder: true)
                            FieldRow(title: "Database", value: appModel.serviceStatus?.dbPath, showPlaceholder: true)
                            FieldRow(title: "Service method", value: appModel.serviceStatus?.preferredProvider, showPlaceholder: true)
                            FieldRow(title: "Latest Capture", value: appModel.serviceStatus?.recentCaptureAt, showPlaceholder: true)
                            FieldRow(title: "Retention", value: appModel.serviceStatus?.retention, showPlaceholder: true)
                        }
                        HStack {
                            Button("Compact Database", systemImage: "archivebox") {
                                confirmCompact = true
                            }
                            Button("Optimize Images...", systemImage: "photo.stack") {
                                confirmOptimizeImages = true
                            }
                            Button("Purge Older Than...", systemImage: "trash") {
                                showManualPurge = true
                            }
                        }
                        .buttonStyle(.bordered)
                        .disabled(appModel.isRunningAction)
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
        .confirmationDialog("Compact the clipmem database?", isPresented: $confirmCompact) {
            Button("Compact Database") {
                Task { await appModel.compactDatabase() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This reclaims SQLite and WAL disk space. Clipboard content is not changed. The operation may need temporary disk space while SQLite rebuilds the database.")
        }
        .confirmationDialog("Optimize stored images?", isPresented: $confirmOptimizeImages) {
            Button("Optimize Images") {
                Task { await appModel.optimizeImages() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This replaces original encoded image bytes with lossless WebP, preserves exact decoded pixels, compacts SQLite afterward to return freed pages to disk, and will never recompress already processed images.")
        }
        .sheet(isPresented: $showManualPurge) {
            ManualPurgeSheet(appModel: appModel, initialDuration: appModel.serviceStatus?.retention)
        }
    }
}
