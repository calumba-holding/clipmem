import SwiftUI

struct SnapshotDetailView: View {
    let detail: SnapshotDetails?
    let fallback: ClipmemItem?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
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
    }

    @ViewBuilder
    private func textSection(_ detail: SnapshotDetails) -> some View {
        VStack(alignment: .leading, spacing: 8) {
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
        VStack(alignment: .leading, spacing: 6) {
            DetailFieldRow(title: "Kind", value: detail.snapshotKind)
            DetailFieldRow(title: "Snapshot ID", value: String(detail.snapshotId))
            DetailFieldRow(title: "SHA-256", value: detail.sha256, lineLimit: 1)
            DetailFieldRow(title: "First Seen", value: detail.firstObservedAt)
            DetailFieldRow(title: "Last Seen", value: detail.lastObservedAt)
            DetailFieldRow(title: "Capture Count", value: String(detail.captureCount))
            DetailFieldRow(title: "Bytes", value: String(detail.totalBytes))
            DetailFieldRow(title: "App Hint", value: detail.lastFrontmostAppName.map { "Copied while in \($0)" })
            DetailFieldRow(title: "Bundle ID", value: detail.lastFrontmostAppBundleId, lineLimit: 1)
            DetailFieldRow(title: "URLs", value: detail.urls.joined(separator: "\n"), lineLimit: 3)
            DetailFieldRow(title: "Files", value: detail.filePaths.joined(separator: "\n"), lineLimit: 3)
        }
    }

    private func representationsSection(_ detail: SnapshotDetails) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Representations")
                .font(.headline)
            ForEach(detail.items) { item in
                VStack(alignment: .leading, spacing: 5) {
                    Text("Item \(item.itemIndex)")
                        .font(.subheadline.weight(.semibold))
                    ForEach(item.representations) { representation in
                        HStack {
                            Text(representation.uti)
                                .lineLimit(1)
                                .truncationMode(.middle)
                                .frame(maxWidth: .infinity, alignment: .leading)
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
        VStack(alignment: .leading, spacing: 8) {
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

private struct DetailFieldRow: View {
    let title: String
    let value: String?
    var lineLimit: Int?

    var body: some View {
        if let value, value.isEmpty == false {
            HStack(alignment: .top, spacing: 12) {
                Text(title)
                    .foregroundStyle(.secondary)
                    .frame(width: 130, alignment: .trailing)
                Text(value)
                    .textSelection(.enabled)
                    .lineLimit(lineLimit)
                    .truncationMode(.middle)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}
