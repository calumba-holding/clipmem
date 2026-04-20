import AppKit

enum MenuBarConfirmation {
    case compactDatabase
    case optimizeImages
    case uninstallService

    var title: String {
        switch self {
        case .compactDatabase:
            "Compact the clipmem database?"
        case .optimizeImages:
            "Optimize stored images?"
        case .uninstallService:
            "Uninstall the clipmem background service?"
        }
    }

    var message: String {
        switch self {
        case .compactDatabase:
            "This reclaims SQLite and WAL disk space. Clipboard content is not changed. The operation may need temporary disk space while SQLite rebuilds the database."
        case .optimizeImages:
            "This replaces original encoded image bytes with lossless WebP, preserves exact decoded pixels, compacts SQLite afterward to return freed pages to disk, and will never recompress already processed images."
        case .uninstallService:
            "This stops clipboard capture. Your saved history is kept. You can reinstall with Setup."
        }
    }

    var confirmButtonTitle: String {
        switch self {
        case .compactDatabase:
            "Compact Database"
        case .optimizeImages:
            "Optimize Images"
        case .uninstallService:
            "Uninstall"
        }
    }

    var cancelButtonTitle: String {
        "Cancel"
    }

    var alertStyle: NSAlert.Style {
        switch self {
        case .compactDatabase, .optimizeImages:
            .warning
        case .uninstallService:
            .critical
        }
    }
}

@MainActor
struct ConfirmationAlertPresenter {
    static func confirm(_ confirmation: MenuBarConfirmation) -> Bool {
        NSApp.activate(ignoringOtherApps: true)

        let alert = NSAlert()
        alert.messageText = confirmation.title
        alert.informativeText = confirmation.message
        alert.alertStyle = confirmation.alertStyle
        alert.addButton(withTitle: confirmation.confirmButtonTitle)
        alert.addButton(withTitle: confirmation.cancelButtonTitle)

        return alert.runModal() == .alertFirstButtonReturn
    }
}
