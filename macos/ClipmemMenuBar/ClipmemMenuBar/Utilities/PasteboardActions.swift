import AppKit
import Foundation
import UniformTypeIdentifiers

enum PasteboardActions {
    @MainActor
    static func copyPlainText(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
    }

    @MainActor
    static func openSingleURL(_ urls: [String]?) {
        guard let value = urls?.first, urls?.count == 1, let url = URL(string: value) else { return }
        NSWorkspace.shared.open(url)
    }

    @MainActor
    static func revealFilePath(_ paths: [String]?) {
        guard let path = paths?.first, paths?.isEmpty == false else { return }
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
    }
}

struct ExportDestination {
    @MainActor
    static func choose(defaultName: String) -> String? {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = defaultName
        panel.canCreateDirectories = true
        panel.title = "Export Representation"
        panel.message = "Choose where to write the selected clipboard representation."
        return panel.runModal() == .OK ? panel.url?.path : nil
    }
}
