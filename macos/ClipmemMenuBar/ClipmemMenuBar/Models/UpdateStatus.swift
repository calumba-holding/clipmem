import Foundation

struct AppVersion: Comparable, Equatable {
    private let components: [Int]

    init?(_ rawValue: String?) {
        guard var value = rawValue?.trimmingCharacters(in: .whitespacesAndNewlines), value.isEmpty == false else {
            return nil
        }
        if value.first == "v" || value.first == "V" {
            value.removeFirst()
        }
        guard value.contains("-") == false else { return nil }

        let parts = value.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.isEmpty == false else { return nil }

        var parsed: [Int] = []
        for part in parts {
            guard part.isEmpty == false, part.allSatisfy(\.isNumber), let number = Int(part) else {
                return nil
            }
            parsed.append(number)
        }
        components = parsed
    }

    static func < (lhs: AppVersion, rhs: AppVersion) -> Bool {
        let count = max(lhs.components.count, rhs.components.count)
        for index in 0..<count {
            let lhsValue = index < lhs.components.count ? lhs.components[index] : 0
            let rhsValue = index < rhs.components.count ? rhs.components[index] : 0
            if lhsValue != rhsValue {
                return lhsValue < rhsValue
            }
        }
        return false
    }
}

struct UpdateCheckResult: Equatable {
    let latestVersion: String
    let releaseURL: URL
    let checkedAt: Date
}

enum InstallOrigin: Equatable {
    case homebrew
    case other

    var prefersHomebrewUpgrade: Bool {
        self == .homebrew
    }

    static func detect(fileManager: FileManager = .default) -> InstallOrigin {
        let homebrewMarkers = [
            "/opt/homebrew/Caskroom/clipmem-app",
            "/usr/local/Caskroom/clipmem-app",
            "/usr/local/Homebrew/Caskroom/clipmem-app"
        ]
        if homebrewMarkers.contains(where: { fileManager.fileExists(atPath: $0) }) {
            return .homebrew
        }
        return .other
    }
}

struct UpdateStatus: Equatable {
    var currentVersion: String
    var latestVersion: String?
    var releaseURL: URL?
    var lastCheckedAt: Date?
    var isChecking = false
    var errorMessage: String?
    var installOrigin: InstallOrigin

    var isUpdateAvailable: Bool {
        guard
            let current = AppVersion(currentVersion),
            let latest = AppVersion(latestVersion)
        else {
            return false
        }
        return latest > current
    }

    var shouldShowHomebrewCommand: Bool {
        isUpdateAvailable && installOrigin.prefersHomebrewUpgrade
    }

    func shouldCheck(now: Date = Date(), interval: TimeInterval = UpdateChecker.checkInterval) -> Bool {
        guard let lastCheckedAt else { return true }
        return now.timeIntervalSince(lastCheckedAt) >= interval
    }

    mutating func beginCheck(manual: Bool) {
        isChecking = true
        if manual {
            errorMessage = nil
        }
    }

    mutating func applySuccess(_ result: UpdateCheckResult?, defaults: UserDefaults = .standard) {
        isChecking = false
        errorMessage = nil
        lastCheckedAt = result?.checkedAt ?? Date()
        if let result {
            latestVersion = result.latestVersion
            releaseURL = result.releaseURL
        } else {
            latestVersion = nil
            releaseURL = nil
        }
        storeCache(defaults: defaults)
    }

    mutating func applyFailure(_ error: Error, manual: Bool) {
        isChecking = false
        if manual {
            errorMessage = UpdateChecker.userVisibleMessage(for: error)
        }
    }

    static func load(
        currentVersion: String = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0",
        defaults: UserDefaults = .standard,
        installOrigin: InstallOrigin = .detect()
    ) -> UpdateStatus {
        let latestVersion = defaults.string(forKey: PreferenceKey.cachedLatestVersion)
        let releaseURL = defaults.string(forKey: PreferenceKey.cachedLatestReleaseURL).flatMap(URL.init(string:))
        let checkedTimestamp = defaults.object(forKey: PreferenceKey.lastUpdateCheckAt) as? Double
        let lastCheckedAt = checkedTimestamp.map(Date.init(timeIntervalSince1970:))
        return UpdateStatus(
            currentVersion: currentVersion,
            latestVersion: latestVersion,
            releaseURL: releaseURL,
            lastCheckedAt: lastCheckedAt,
            installOrigin: installOrigin
        )
    }

    func storeCache(defaults: UserDefaults = .standard) {
        defaults.set(latestVersion, forKey: PreferenceKey.cachedLatestVersion)
        defaults.set(releaseURL?.absoluteString, forKey: PreferenceKey.cachedLatestReleaseURL)
        defaults.set(lastCheckedAt?.timeIntervalSince1970, forKey: PreferenceKey.lastUpdateCheckAt)
    }
}
