import SwiftUI

struct SnapshotDetailView: View {
    let detail: SnapshotDetails?
    let fallback: ClipmemItem?
    var isLoading: Bool = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Spacing.xl) {
                if let detail {
                    textSection(detail)
                    Divider()
                    metadataSection(detail)
                    Divider()
                    representationsSection(detail)
                    Divider()
                    eventsSection(detail)
                } else if let fallback {
                    Text(fallback.displayText)
                        .textSelection(.enabled)
                        .font(DesignType.bodyPrimary)
                    Text("Select an item to load full snapshot detail.")
                        .foregroundStyle(.secondary)
                } else if isLoading {
                    loadingSkeleton
                } else {
                    EmptyStateView(title: "No Selection", detail: "Select a clipboard item to inspect it.", symbol: "sidebar.right")
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .overlay {
            if isLoading && detail == nil && fallback != nil {
                loadingSkeleton
                    .padding()
            }
        }
    }

    @ViewBuilder
    private func textSection(_ detail: SnapshotDetails) -> some View {
        VStack(alignment: .leading, spacing: Spacing.sm) {
            HStack {
                Text("Content")
                    .font(DesignType.sectionHeader)
                Spacer()
                if let text = bestText(from: detail), !text.isEmpty {
                    Button("Copy", systemImage: "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(text, forType: .string)
                    }
                    .buttonStyle(.borderless)
                    .controlSize(.small)
                    .foregroundStyle(.secondary)
                }
            }
            if let text = bestText(from: detail) {
                Text(text)
                    .textSelection(.enabled)
                    .font(DesignType.mono)
                    .lineLimit(nil)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(Spacing.md)
                    .background(Color(.textBackgroundColor), in: .rect(cornerRadius: DesignRadius.md))
            } else {
                ContentUnavailableView("No Extracted Text", systemImage: "shippingbox", description: Text("This snapshot appears to be binary, image, PDF, or otherwise has no extracted text. Metadata and export actions are available."))
            }
        }
    }

    private func bestText(from detail: SnapshotDetails) -> String? {
        [detail.bestText, detail.previewText, detail.textSummary]
            .compactMap { $0 }
            .first(where: { $0.isEmpty == false })
    }

    private func metadataSection(_ detail: SnapshotDetails) -> some View {
        GroupBox("Metadata") {
            Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
                FieldRow(title: "Kind", value: detail.snapshotKind)
                FieldRow(title: "Snapshot ID", value: String(detail.snapshotId))
                FieldRow(title: "Content fingerprint", value: detail.sha256, lineLimit: 1)
                FieldRow(title: "First Seen", value: DisplayFormatters.localTimestamp(detail.firstObservedAt))
                FieldRow(title: "Last Seen", value: DisplayFormatters.localTimestamp(detail.lastObservedAt))
                FieldRow(title: "Capture Count", value: String(detail.captureCount))
                FieldRow(title: "Bytes", value: DisplayFormatters.byteCount(detail.totalBytes) ?? String(detail.totalBytes))
                FieldRow(title: "App Hint", value: detail.lastFrontmostAppName.map { "Copied while in \($0)" })
                FieldRow(title: "App identifier", value: detail.lastFrontmostAppBundleId, lineLimit: 1)
                FieldRow(title: "URLs", value: detail.urls.joined(separator: "\n"), lineLimit: 3)
                FieldRow(title: "Files", value: detail.filePaths.joined(separator: "\n"), lineLimit: 3)
            }
        }
    }

    private func representationsSection(_ detail: SnapshotDetails) -> some View {
        GroupBox("Data Formats") {
            VStack(alignment: .leading, spacing: Spacing.md) {
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
                            .font(DesignType.rowMeta)
                            .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    private func eventsSection(_ detail: SnapshotDetails) -> some View {
        GroupBox("Recent Events") {
            VStack(alignment: .leading, spacing: Spacing.sm) {
                ForEach(detail.recentEvents) { event in
                    HStack {
                        Text("#\(event.eventId)")
                            .monospacedDigit()
                        Text(DisplayFormatters.relativeTimestamp(event.observedAt) ?? event.observedAt)
                            .help(DisplayFormatters.localTimestamp(event.observedAt) ?? event.observedAt)
                        if let app = event.frontmostAppName {
                            Text("Copied while in \(app)")
                                .lineLimit(1)
                                .truncationMode(.tail)
                        }
                    }
                    .font(DesignType.rowMeta)
                    .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var loadingSkeleton: some View {
        VStack(alignment: .leading, spacing: Spacing.xl) {
            VStack(alignment: .leading, spacing: Spacing.sm) {
                Text("Content")
                    .font(DesignType.sectionHeader)
                RoundedRectangle(cornerRadius: DesignRadius.sm)
                    .fill(.quaternary)
                    .frame(height: 80)
            }
            Divider()
            VStack(alignment: .leading, spacing: Spacing.sm) {
                Text("Metadata")
                    .font(DesignType.sectionHeader)
                ForEach(0..<4, id: \.self) { _ in
                    RoundedRectangle(cornerRadius: DesignRadius.sm)
                        .fill(.quaternary)
                        .frame(height: 16)
                        .frame(maxWidth: 300)
                }
            }
        }
        .redacted(reason: .placeholder)
    }
}
