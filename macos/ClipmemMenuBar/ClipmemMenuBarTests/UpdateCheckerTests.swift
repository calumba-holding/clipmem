import Foundation
import Testing
@testable import ClipmemMenuBar

struct UpdateCheckerTests {
    @Test func semanticVersionsCompareNumerically() throws {
        let newer = try #require(AppVersion("v0.2.10"))
        let older = try #require(AppVersion("0.2.6"))

        #expect(newer > older)
    }

    @Test func equalVersionsDoNotShowUpdate() {
        let status = UpdateStatus(
            currentVersion: "0.2.6",
            latestVersion: "v0.2.6",
            releaseURL: URL(string: "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.6"),
            installOrigin: .homebrew
        )

        #expect(status.isUpdateAvailable == false)
    }

    @Test func prereleaseTagsAreIgnored() {
        #expect(AppVersion("v0.2.7-beta.1") == nil)
        #expect(AppVersion("0.2.7-rc.1") == nil)
    }

    @Test func githubReleaseDecodesStableFixture() throws {
        let data = Data(
            """
            {
              "tag_name": "v0.2.10",
              "html_url": "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10",
              "prerelease": false,
              "draft": false
            }
            """.utf8
        )
        let release = try JSONDecoder().decode(GitHubReleaseResponse.self, from: data)
        let result = try #require(release.stableResult(checkedAt: Date(timeIntervalSince1970: 1_800_000_000)))

        #expect(result.latestVersion == "v0.2.10")
        #expect(result.releaseURL.absoluteString == "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10")
    }

    @Test func draftAndPrereleaseResponsesDoNotProduceUpdates() {
        let prerelease = GitHubReleaseResponse(
            tagName: "v0.2.10-beta.1",
            htmlURL: URL(string: "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10-beta.1")!,
            prerelease: true,
            draft: false
        )
        let draft = GitHubReleaseResponse(
            tagName: "v0.2.10",
            htmlURL: URL(string: "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10")!,
            prerelease: false,
            draft: true
        )

        #expect(prerelease.stableResult(checkedAt: Date()) == nil)
        #expect(draft.stableResult(checkedAt: Date()) == nil)
    }

    @Test func newerLatestReleaseIsUpdateAvailable() {
        let status = UpdateStatus(
            currentVersion: "0.2.6",
            latestVersion: "v0.2.10",
            releaseURL: URL(string: "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10")!,
            installOrigin: .homebrew
        )

        #expect(status.isUpdateAvailable)
        #expect(status.shouldShowHomebrewCommand)
    }

    @Test func failedBackgroundCheckPreservesCachedStateWithoutSurfacingError() {
        var status = UpdateStatus(
            currentVersion: "0.2.6",
            latestVersion: "v0.2.10",
            releaseURL: URL(string: "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10")!,
            lastCheckedAt: Date(timeIntervalSince1970: 1_800_000_000),
            installOrigin: .homebrew
        )

        status.beginCheck(manual: false)
        status.applyFailure(UpdateCheckError.badStatus(500), manual: false)

        #expect(status.isChecking == false)
        #expect(status.latestVersion == "v0.2.10")
        #expect(status.releaseURL?.absoluteString == "https://github.com/tristanmanchester/clipmem/releases/tag/v0.2.10")
        #expect(status.errorMessage == nil)
    }

    @Test func manualCheckFailureShowsNonBlockingError() {
        var status = UpdateStatus(currentVersion: "0.2.6", installOrigin: .homebrew)

        status.beginCheck(manual: true)
        status.applyFailure(UpdateCheckError.badStatus(403), manual: true)

        #expect(status.isChecking == false)
        #expect(status.errorMessage == "GitHub returned HTTP 403.")
    }
}
