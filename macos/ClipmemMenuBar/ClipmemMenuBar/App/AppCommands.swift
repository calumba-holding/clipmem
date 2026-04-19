import SwiftUI

enum WindowID: String {
    case history = "history"
    case quickRecall = "quick-recall"
}

enum PreferenceKey {
    static let binaryPathOverride = "binaryPathOverride"
    static let databasePathOverride = "databasePathOverride"
    static let defaultRecentHours = "defaultRecentHours"
    static let defaultQueryMode = "defaultQueryMode"
    static let hotkeyEnabled = "hotkeyEnabled"
    static let didInstallSelfIgnore = "didInstallSelfIgnore"
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
}
