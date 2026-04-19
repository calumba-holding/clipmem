import AppKit
import SwiftUI

enum WindowID: String {
    case history = "history"
    case quickRecall = "quick-recall"

    var title: String {
        switch self {
        case .history: "History"
        case .quickRecall: "Quick Recall"
        }
    }
}

enum PreferenceKey {
    static let binaryPathOverride = "binaryPathOverride"
    static let databasePathOverride = "databasePathOverride"
    static let defaultRecentHours = "defaultRecentHours"
    static let defaultQueryMode = "defaultQueryMode"
    static let hotkeyEnabled = "hotkeyEnabled"
    static let launchAtLoginEnabled = "launchAtLoginEnabled"
    static let didConfigureLaunchAtLogin = "didConfigureLaunchAtLogin"
    static let didInstallSelfIgnore = "didInstallSelfIgnore"
    static let cachedLatestVersion = "cachedLatestVersion"
    static let cachedLatestReleaseURL = "cachedLatestReleaseURL"
    static let lastUpdateCheckAt = "lastUpdateCheckAt"
}

extension UserDefaults {
    var clipmemDefaultHours: Int {
        let value = integer(forKey: PreferenceKey.defaultRecentHours)
        return value == 0 ? 24 : value
    }

    var clipmemDefaultMode: QueryMode {
        let rawValue = string(forKey: PreferenceKey.defaultQueryMode) ?? QueryMode.recent.rawValue
        return QueryMode(rawValue: rawValue) ?? .recent
    }

    var clipmemHotkeyEnabled: Bool {
        if object(forKey: PreferenceKey.hotkeyEnabled) == nil {
            return true
        }
        return bool(forKey: PreferenceKey.hotkeyEnabled)
    }

    var clipmemLaunchAtLoginEnabled: Bool {
        if object(forKey: PreferenceKey.launchAtLoginEnabled) == nil {
            return LoginItemController.bundleDefaultEnabled
        }
        return bool(forKey: PreferenceKey.launchAtLoginEnabled)
    }
}

enum WindowActivation {
    @MainActor
    static func openWindow(_ openWindow: OpenWindowAction, id: WindowID) {
        openWindow(id: id.rawValue)
        bringAppForward(target: .window(id))
    }

    @MainActor
    static func openSettings(_ openSettings: OpenSettingsAction) {
        openSettings()
        bringAppForward(target: .settings)
    }

    @MainActor
    static func bringAppForward(target: Target? = nil) {
        activateNow(target: target)
        DispatchQueue.main.async {
            activateNow(target: target)
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) {
            activateNow(target: target)
        }
    }

    @MainActor
    private static func activateNow(target: Target?) {
        NSApp.activate(ignoringOtherApps: true)
        let windows = NSApp.windows.filter { window in
            window.isVisible && window.canBecomeKey && (target?.matches(window) ?? true)
        }
        for window in windows {
            window.orderFrontRegardless()
        }
    }

    enum Target {
        case window(WindowID)
        case settings

        @MainActor
        func matches(_ window: NSWindow) -> Bool {
            switch self {
            case .window(let id):
                return window.title == id.title
            case .settings:
                let identifier = window.identifier?.rawValue ?? ""
                return identifier == "com_apple_SwiftUI_Settings_window" || [
                    "General",
                    "Capture",
                    "Ignored Apps",
                    "Privacy"
                ].contains(window.title)
            }
        }
    }
}
