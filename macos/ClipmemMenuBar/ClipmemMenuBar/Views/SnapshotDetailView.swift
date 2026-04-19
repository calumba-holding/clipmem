import SwiftUI

struct SnapshotDetailView: View {
    let detail: SnapshotDetails?
    let fallback: ClipmemItem?
    var isLoading: Bool = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Spacing.lg) {
                if let detail {
                    textSection(detail)
                    metadataSection(detail)
                    representationsSection(detail)
                    eventsSection(detail)
                } else if let fallback {
                    Text(fallback.displayText)
                        .textSelection(.enabled)
                        .font(.body)
                    Text("Select an item to load full snapshot detail.")
                        .foregroundStyle(.secondary)
                } else {
                    EmptyStateView(title: "No Selection", detail: "Select a clipboard item to inspect it.", symbol: "sidebar.right")
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .overlay {
            if isLoading && detail == nil && fallback != nil {
                ProgressView()
            }
        }
    }

    @ViewBuilder
    private func textSection(_ detail: SnapshotDetails) -> some View {
        VStack(alignment: .leading, spacing: Spacing.sm) {
            Text("Content")
                .font(.headline)
            if let text = [detail.bestText, detail.previewText, detail.textSummary].compactMap({ $0 }).first(where: { $0.isEmpty == false }) {
                Text(text)
                    .textSelection(.enabled)
                    .font(.body.monospaced())
                    .lineLimit(nil)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                ContentUnavailableView("No Extracted Text", systemImage: "shippingbox", description: Text("This snapshot appears to be binary, image, PDF, or otherwise has no extracted text. Metadata and export actions are available."))
            }
        }
    }

    private func metadataSection(_ detail: SnapshotDetails) -> some View {
        Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
            FieldRow(title: "Kind", value: detail.snapshotKind)
            FieldRow(title: "Snapshot ID", value: String(detail.snapshotId))
            FieldRow(title: "Content fingerprint", value: detail.sha256, lineLimit: 1)
            FieldRow(title: "First Seen", value: detail.firstObservedAt)
            FieldRow(title: "Last Seen", value: detail.lastObservedAt)
            FieldRow(title: "Capture Count", value: String(detail.captureCount))
            FieldRow(title: "Bytes", value: String(detail.totalBytes))
            FieldRow(title: "App Hint", value: detail.lastFrontmostAppName.map { "Copied while in \($0)" })
            FieldRow(title: "App identifier", value: detail.lastFrontmostAppBundleId, lineLimit: 1)
            FieldRow(title: "URLs", value: detail.urls.joined(separator: "\n"), lineLimit: 3)
            FieldRow(title: "Files", value: detail.filePaths.joined(separator: "\n"), lineLimit: 3)
        }
    }

    private func representationsSection(_ detail: SnapshotDetails) -> some View {
        VStack(alignment: .leading, spacing: Spacing.sm) {
            Text("Data Formats")
                .font(.headline)
            ForEach(detail.items) { item in
                VStack(alignment: .leading, spacing: Spacing.xs) {
                    Text("Item \(item.itemIndex)")
                        .font(.subheadline.weight(.semibold))
                    ForEach(item.representations) { representation in
                        HStack {
                            Text(humanReadableType(representation.uti))
                                .lineLimit(1)
                                .truncationMode(.middle)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .help(representation.uti)
                            Text(representation.kind ?? "unknown")
                                .lineLimit(1)
                            Text("\(representation.byteLen) bytes")
                                .lineLimit(1)
                        }
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func eventsSection(_ detail: SnapshotDetails) -> some View {
        VStack(alignment: .leading, spacing: Spacing.sm) {
            Text("Recent Events")
                .font(.headline)
            ForEach(detail.recentEvents) { event in
                HStack {
                    Text("#\(event.eventId)")
                        .monospacedDigit()
                    Text(event.observedAt)
                    if let app = event.frontmostAppName {
                        Text("Copied while in \(app)")
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
        }
    }
}
