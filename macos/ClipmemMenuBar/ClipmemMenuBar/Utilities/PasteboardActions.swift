import AppKit
import Foundation
import UniformTypeIdentifiers

enum LinkPresentationBadge: String, Equatable, Sendable {
    case url
    case file
    case directory
    case links
}

enum LinkTargetResolution: Equatable, Sendable {
    case web(URL)
    case file(URL, isDirectory: Bool)
    case unsupported

    var badge: LinkPresentationBadge? {
        switch self {
        case .web:
            return .url
        case .file(_, let isDirectory):
            return isDirectory ? .directory : .file
        case .unsupported:
            return nil
        }
    }
}

enum LinkTargetResolver {
    static func classify(_ rawTarget: String, fileManager: FileManager = .default) -> LinkTargetResolution {
        let target = rawTarget.trimmingCharacters(in: .whitespacesAndNewlines)
        guard target.isEmpty == false else { return .unsupported }

        if let url = URL(string: target), let scheme = url.scheme?.lowercased() {
            switch scheme {
            case "http", "https":
                return .web(url)
            case "file":
                return fileResolution(for: url, fileManager: fileManager)
            default:
                return .unsupported
            }
        }

        guard target.hasPrefix("/") else { return .unsupported }
        return fileResolution(for: URL(fileURLWithPath: target), fileManager: fileManager)
    }

    static func presentationBadge(for rawTarget: String) -> LinkPresentationBadge? {
        let target = rawTarget.trimmingCharacters(in: .whitespacesAndNewlines)
        guard target.isEmpty == false else { return nil }

        if let url = URL(string: target), let scheme = url.scheme?.lowercased() {
            switch scheme {
            case "http", "https":
                return .url
            case "file":
                return .file
            default:
                return nil
            }
        }

        return target.hasPrefix("/") ? .file : nil
    }

    static func presentationBadge(
        urls: [String]?,
        filePaths: [String]?,
        markdownLinks: [MarkdownRenderedLink],
        fileManager: FileManager = .default,
        resolveFilePathDirectories: Bool = true
    ) -> LinkPresentationBadge? {
        var badges: Set<LinkPresentationBadge> = []

        for url in urls ?? [] {
            if let badge = presentationBadge(for: url) {
                badges.insert(badge)
            }
        }

        for path in filePaths ?? [] {
            let badge = resolveFilePathDirectories
                ? classify(path, fileManager: fileManager).badge
                : presentationBadge(for: path)
            if let badge {
                badges.insert(badge)
            }
        }

        for link in markdownLinks {
            if let badge = link.badge {
                badges.insert(badge)
            }
        }

        if badges.isEmpty {
            return nil
        }
        if badges.count == 1 {
            return badges.first
        }
        return .links
    }

    private static func fileResolution(for url: URL, fileManager: FileManager) -> LinkTargetResolution {
        let fileURL = url.isFileURL ? url.standardizedFileURL : URL(fileURLWithPath: url.path)
        var isDirectory: ObjCBool = false
        let exists = fileManager.fileExists(atPath: fileURL.path, isDirectory: &isDirectory)
        return .file(fileURL, isDirectory: exists && isDirectory.boolValue)
    }
}

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

    @MainActor
    @discardableResult
    static func openLinkTarget(_ target: LinkTargetResolution) -> Bool {
        switch target {
        case .web(let url):
            NSWorkspace.shared.open(url)
            return true
        case .file(let url, _):
            NSWorkspace.shared.activateFileViewerSelecting([url])
            return true
        case .unsupported:
            return false
        }
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
