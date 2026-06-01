import AppKit
import SwiftUI

struct ItemActionButtons: View {
    let detail: SnapshotDetails
    let appModel: AppModel

    var body: some View {
        GroupBox("More Actions") {
            VStack(alignment: .leading, spacing: Spacing.sm) {
                Button("Open URL", systemImage: "safari") {
                    PasteboardActions.openSingleURL(detail.urls)
                }
                .disabled(detail.urls.count != 1)

                Button("Reveal File", systemImage: "finder") {
                    PasteboardActions.revealFilePath(detail.filePaths)
                }
                .disabled(detail.filePaths.isEmpty)

                Menu("Export Representation", systemImage: "square.and.arrow.down") {
                    ForEach(detail.items) { clipboardItem in
                        ForEach(clipboardItem.representations) { representation in
                            Button("\(clipboardItem.itemIndex): \(humanReadableType(representation.uti))") {
                                export(clipboardItem: clipboardItem, representation: representation)
                            }
                        }
                    }
                }
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }

    private func export(clipboardItem: ClipboardItemDetail, representation: ClipboardRepresentation) {
        let defaultName = "clipmem-\(detail.snapshotId)-\(clipboardItem.itemIndex)"
        guard let destination = ExportDestination.choose(defaultName: defaultName) else { return }
        Task {
            do {
                _ = try await appModel.client.export(
                    snapshotID: detail.snapshotId,
                    itemIndex: clipboardItem.itemIndex,
                    uti: representation.uti,
                    destination: destination,
                    force: true
                )
                appModel.lastError = nil
                appModel.actionMessage = "Exported successfully"
            } catch {
                appModel.lastError = UserError(error)
            }
        }
    }
}
