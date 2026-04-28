import Foundation
import Testing
@testable import ClipmemMenuBar

struct MarkdownLinkActionTests {
    @Test func classifiesHttpsTargetsAsWebLinks() throws {
        let resolution = LinkTargetResolver.classify("https://example.com/path")

        guard case .web(let url) = resolution else {
            Issue.record("Expected web target")
            return
        }
        #expect(url.absoluteString == "https://example.com/path")
        #expect(resolution.badge == .url)
    }

    @Test func classifiesHttpTargetsAsWebLinks() throws {
        let resolution = LinkTargetResolver.classify("http://example.com")

        guard case .web(let url) = resolution else {
            Issue.record("Expected web target")
            return
        }
        #expect(url.absoluteString == "http://example.com")
        #expect(resolution.badge == .url)
    }

    @Test func classifiesFileURLsAsFinderTargets() throws {
        let resolution = LinkTargetResolver.classify("file:///tmp/example.txt")

        guard case .file(let url, let isDirectory) = resolution else {
            Issue.record("Expected file target")
            return
        }
        #expect(url.path == "/tmp/example.txt")
        #expect(isDirectory == false)
        #expect(resolution.badge == .file)
    }

    @Test func classifiesAbsolutePathsAsFinderTargets() throws {
        let resolution = LinkTargetResolver.classify("/tmp/example.txt")

        guard case .file(let url, let isDirectory) = resolution else {
            Issue.record("Expected file target")
            return
        }
        #expect(url.path == "/tmp/example.txt")
        #expect(isDirectory == false)
        #expect(resolution.badge == .file)
    }

    @Test func classifiesExistingDirectoriesForDirectoryBadges() throws {
        let directoryURL = FileManager.default.temporaryDirectory
        let resolution = LinkTargetResolver.classify(directoryURL.path)

        guard case .file(let url, let isDirectory) = resolution else {
            Issue.record("Expected file target")
            return
        }
        #expect(url.standardizedFileURL.path == directoryURL.standardizedFileURL.path)
        #expect(isDirectory == true)
        #expect(resolution.badge == .directory)
    }

    @Test func rejectsMailtoTargets() {
        let resolution = LinkTargetResolver.classify("mailto:test@example.com")

        #expect(resolution == .unsupported)
        #expect(resolution.badge == nil)
    }

    @Test func rejectsRelativeTargets() {
        let resolution = LinkTargetResolver.classify("docs/file.md")

        #expect(resolution == .unsupported)
        #expect(resolution.badge == nil)
    }

    @Test func rejectsInvalidTargets() {
        let resolution = LinkTargetResolver.classify("")

        #expect(resolution == .unsupported)
        #expect(resolution.badge == nil)
    }

    @Test func derivesURLBadgeFromExistingMetadata() {
        let badge = LinkTargetResolver.presentationBadge(
            urls: ["https://github.com/openclaw/openclaw/pull/57484"],
            filePaths: nil,
            markdownLinks: []
        )

        #expect(badge == .url)
    }

    @Test func derivesFileBadgeFromExistingMetadata() {
        let badge = LinkTargetResolver.presentationBadge(
            urls: nil,
            filePaths: ["/tmp/example.txt"],
            markdownLinks: []
        )

        #expect(badge == .file)
    }

    @Test func derivesDirectoryBadgeFromExistingMetadata() {
        let badge = LinkTargetResolver.presentationBadge(
            urls: nil,
            filePaths: [FileManager.default.temporaryDirectory.path],
            markdownLinks: []
        )

        #expect(badge == .directory)
    }

    @Test func derivesMixedBadgeForMultipleTargetTypes() {
        let badge = LinkTargetResolver.presentationBadge(
            urls: ["https://example.com"],
            filePaths: ["/tmp/example.txt"],
            markdownLinks: []
        )

        #expect(badge == .links)
    }
}
