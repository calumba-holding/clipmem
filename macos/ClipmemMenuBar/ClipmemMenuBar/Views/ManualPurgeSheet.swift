import SwiftUI

struct ManualPurgeSheet: View {
    let appModel: AppModel

    @Environment(\.dismiss) private var dismiss
    @State private var olderThan: String
    @State private var preview: PurgeOutput?
    @State private var previewedOlderThan: String?
    @State private var localError: UserError?

    init(appModel: AppModel, initialDuration: String?) {
        self.appModel = appModel
        _olderThan = State(initialValue: Self.defaultDuration(from: initialDuration))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.lg) {
            VStack(alignment: .leading, spacing: Spacing.sm) {
                Text("Purge Older Than")
                    .font(.title3.weight(.semibold))
                Text("Preview matching clipboard snapshots before deleting them from the local archive.")
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: Spacing.xs) {
                TextField("Older than", text: $olderThan)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        Task { await previewPurge() }
                    }
                Text("Use values like 30d, 12h, or 15m.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if let error = localError {
                ErrorBanner(message: error.message, recovery: error.recovery)
            }

            previewSummary

            HStack {
                if appModel.isRunningAction {
                    ProgressView()
                        .controlSize(.small)
                }
                Spacer()
                Button("Cancel") {
                    dismiss()
                }
                Button("Preview", systemImage: "eye") {
                    Task { await previewPurge() }
                }
                .disabled(trimmedOlderThan.isEmpty || appModel.isRunningAction)
                Button("Purge", role: .destructive) {
                    Task { await purge() }
                }
                .disabled(canPurge == false)
            }
        }
        .padding()
        .frame(width: 440)
        .onChange(of: olderThan) {
            preview = nil
            previewedOlderThan = nil
            localError = nil
        }
    }

    @ViewBuilder
    private var previewSummary: some View {
        if let preview {
            VStack(alignment: .leading, spacing: Spacing.md) {
                if preview.snapshotCount == 0 {
                    Label("No matching snapshots will be deleted.", systemImage: "checkmark.circle")
                        .foregroundStyle(.secondary)
                } else {
                    Label("This purge will permanently delete matching archive data.", systemImage: "trash")
                        .foregroundStyle(.red)
                }

                Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                    FieldRow(title: "Threshold", value: previewedOlderThan, showPlaceholder: true)
                    FieldRow(title: "Snapshots", value: String(preview.snapshotCount))
                    FieldRow(title: "Items", value: String(preview.itemCount))
                    FieldRow(title: "Representations", value: String(preview.representationCount))
                    FieldRow(title: "Capture events", value: String(preview.captureEventCount))
                    FieldRow(title: "Bytes", value: DisplayFormatters.byteCount(preview.totalBytes) ?? "\(preview.totalBytes) bytes")
                }
            }
            .padding(Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.4), in: .rect(cornerRadius: Spacing.sm))
        } else {
            ContentUnavailableView {
                Label("Preview Required", systemImage: "eye")
            } description: {
                Text("Run a preview to see the deletion counts before purging.")
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, Spacing.lg)
        }
    }

    private var canPurge: Bool {
        guard appModel.isRunningAction == false else { return false }
        guard let preview, preview.snapshotCount > 0 else { return false }
        return previewedOlderThan == trimmedOlderThan
    }

    private var trimmedOlderThan: String {
        olderThan.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func previewPurge() async {
        let threshold = trimmedOlderThan
        let report = await appModel.previewPurge(olderThan: threshold)
        if let report {
            preview = report
            previewedOlderThan = threshold
            localError = nil
        } else {
            preview = nil
            previewedOlderThan = nil
            localError = appModel.lastError ?? UserError(message: "Could not preview purge.")
        }
    }

    private func purge() async {
        guard canPurge else { return }
        if await appModel.purge(olderThan: trimmedOlderThan) != nil {
            dismiss()
        } else {
            localError = appModel.lastError ?? UserError(message: "Could not purge old snapshots.")
        }
    }

    private static func defaultDuration(from value: String?) -> String {
        let duration = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard duration.isEmpty == false, duration.localizedCaseInsensitiveCompare("forever") != .orderedSame else {
            return "30d"
        }
        return duration
    }
}
