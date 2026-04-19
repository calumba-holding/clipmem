import Foundation
import ServiceManagement

enum LoginItemStatus: Equatable {
    case enabled
    case disabled
    case requiresApproval
    case unavailable

    var message: String? {
        switch self {
        case .enabled:
            nil
        case .disabled:
            "Clipmem will not open automatically when you log in."
        case .requiresApproval:
            "Enable Clipmem in System Settings > General > Login Items."
        case .unavailable:
            "Launch at login is unavailable for this build."
        }
    }
}

enum LoginItemController {
    static var bundleDefaultEnabled: Bool {
        guard let value = Bundle.main.object(forInfoDictionaryKey: "ClipmemDefaultLaunchAtLogin") else {
            return false
        }
        if let boolValue = value as? Bool {
            return boolValue
        }
        if let stringValue = value as? String {
            return ["1", "true", "yes"].contains(stringValue.lowercased())
        }
        return false
    }

    @MainActor
    static func status() -> LoginItemStatus {
        switch SMAppService.mainApp.status {
        case .enabled:
            return .enabled
        case .notRegistered:
            return .disabled
        case .requiresApproval:
            return .requiresApproval
        case .notFound:
            return .unavailable
        @unknown default:
            return .unavailable
        }
    }

    @MainActor
    static func setEnabled(_ enabled: Bool) throws {
        if enabled {
            if SMAppService.mainApp.status != .enabled {
                try SMAppService.mainApp.register()
            }
        } else if SMAppService.mainApp.status != .notRegistered {
            try SMAppService.mainApp.unregister()
        }
    }
}
