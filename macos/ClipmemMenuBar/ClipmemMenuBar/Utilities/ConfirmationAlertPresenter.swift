import AppKit

enum MenuBarConfirmation {
    case compactDatabase
    case optimizeImages
    case uninstallService

    var title: String {
        switch self {
        case .compactDatabase:
            "Compact database?"
        case .optimizeImages:
            "Compress stored images?"
        case .uninstallService:
            "Uninstall the clipmem background service?"
        }
    }

    var message: String {
        switch self {
        case .compactDatabase:
            "This reclaims unused SQLite and WAL disk space without deleting clipboard history. The operation may need temporary disk space while SQLite rebuilds the database."
        case .optimizeImages:
            "Clipmem converts eligible screenshots and images to lossless WebP only when it saves space. Image content stays visually identical, already processed images are skipped, and the database is compacted afterward."
        case .uninstallService:
            "This stops clipboard capture. Your saved history is kept. You can reinstall with Setup."
        }
    }

    var confirmButtonTitle: String {
        switch self {
        case .compactDatabase:
            "Compact Database"
        case .optimizeImages:
            "Compress Images"
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
