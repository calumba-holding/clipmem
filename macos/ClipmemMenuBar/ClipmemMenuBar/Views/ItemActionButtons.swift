import AppKit
import SwiftUI

struct ItemActionButtons: View {
    let item: ClipmemItem?
    let detail: SnapshotDetails?
    let appModel: AppModel
    var onForgot: (() async -> Void)?

    @State private var confirmForget = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button("Restore Snapshot", systemImage: "arrow.uturn.backward.square") {
                guard let item else { return }
                Task { await appModel.restore(item) }
            }
            .disabled(item == nil)

            Button("Copy Plain Text", systemImage: "doc.on.doc") {
                let text = detail?.bestText ?? item?.bestText ?? ""
                PasteboardActions.copyPlainText(text)
            }
            .disabled((detail?.bestText ?? item?.bestText ?? "").isEmpty)

            Button("Open URL", systemImage: "safari") {
                PasteboardActions.openSingleURL(detail?.urls ?? item?.urls)
            }
            .disabled((detail?.urls ?? item?.urls ?? []).count != 1)

            Button("Reveal File", systemImage: "finder") {
                PasteboardActions.revealFilePath(detail?.filePaths ?? item?.filePaths)
            }
            .disabled((detail?.filePaths ?? item?.filePaths ?? []).isEmpty)

            Menu("Export Representation", systemImage: "square.and.arrow.down") {
                if let detail {
                    ForEach(detail.items) { clipboardItem in
                        ForEach(clipboardItem.representations) { representation in
                            Button("\(clipboardItem.itemIndex): \(representation.uti)") {
                                export(clipboardItem: clipboardItem, representation: representation)
                            }
                        }
                    }
                } else {
                    Text("Load detail first")
                }
            }
            .disabled(detail == nil)

            Divider()

            Button("Forget Snapshot", systemImage: "trash", role: .destructive) {
                confirmForget = true
            }
            .disabled(item == nil)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .confirmationDialog("Forget this snapshot?", isPresented: $confirmForget) {
            Button("Forget", role: .destructive) {
                Task {
                    if let onForgot {
                        await onForgot()
                    } else if let item {
                        await appModel.forget(item)
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("clipmem deduplicates snapshots. Forgetting removes all capture events for this exact stored content.")
        }
    }

    private func export(clipboardItem: ClipboardItemDetail, representation: ClipboardRepresentation) {
        guard let detail else { return }
        let defaultName = "clipmem-\(detail.snapshotId)-\(clipboardItem.itemIndex)"
        guard let destination = ExportDestination.choose(defaultName: defaultName) else { return }
        Task {
            do {
                _ = try await appModel.client.export(
                    snapshotID: detail.snapshotId,
                    itemIndex: clipboardItem.itemIndex,
                    uti: representation.uti,
                    destination: destination,
                    force: false
                )
                appModel.lastErrorMessage = nil
            } catch {
                appModel.lastErrorMessage = error.localizedDescription
            }
        }
    }
}
