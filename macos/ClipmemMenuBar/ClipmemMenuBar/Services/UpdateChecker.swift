import Foundation

struct UpdateChecker {
    static let checkInterval: TimeInterval = 12 * 60 * 60
    static let homebrewUpgradeCommand = "brew update && brew upgrade tristanmanchester/tap/clipmem && brew upgrade --cask tristanmanchester/tap/clipmem-app"

    private let latestReleaseURL = URL(string: "https://api.github.com/repos/tristanmanchester/clipmem/releases/latest")!
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func latestStableRelease(now: Date = Date()) async throws -> UpdateCheckResult? {
        var request = URLRequest(url: latestReleaseURL)
        request.timeoutInterval = 8
        request.setValue("clipmem-menubar-update-checker", forHTTPHeaderField: "User-Agent")
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")

        let (data, response) = try await session.data(for: request)
        if let httpResponse = response as? HTTPURLResponse, (200..<300).contains(httpResponse.statusCode) == false {
            throw UpdateCheckError.badStatus(httpResponse.statusCode)
        }

        let release = try JSONDecoder().decode(GitHubReleaseResponse.self, from: data)
        return release.stableResult(checkedAt: now)
    }

    static func userVisibleMessage(for error: Error) -> String {
        if let updateError = error as? UpdateCheckError {
            return updateError.localizedDescription
        }
        return "Could not check for updates: \(error.localizedDescription)"
    }
}

enum UpdateCheckError: LocalizedError, Equatable {
    case badStatus(Int)

    var errorDescription: String? {
        switch self {
        case .badStatus(let statusCode):
            "GitHub returned HTTP \(statusCode)."
        }
    }
}

struct GitHubReleaseResponse: Decodable, Equatable {
    let tagName: String
    let htmlURL: URL
    let prerelease: Bool
    let draft: Bool

    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case htmlURL = "html_url"
        case prerelease
        case draft
    }

    func stableResult(checkedAt: Date) -> UpdateCheckResult? {
        guard draft == false, prerelease == false, AppVersion(tagName) != nil else {
            return nil
        }
        return UpdateCheckResult(latestVersion: tagName, releaseURL: htmlURL, checkedAt: checkedAt)
    }
}
