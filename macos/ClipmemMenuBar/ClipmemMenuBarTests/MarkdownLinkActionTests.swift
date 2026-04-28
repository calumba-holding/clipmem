import AppKit
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

    @Test func canSkipDirectoryResolutionForRenderPathBadges() {
        let badge = LinkTargetResolver.presentationBadge(
            urls: nil,
            filePaths: [FileManager.default.temporaryDirectory.path],
            markdownLinks: [],
            resolveFilePathDirectories: false
        )

        #expect(badge == .file)
    }

    @Test func resolvesMarkdownDirectoryLinksOnAsyncBadgePath() {
        let directoryPath = FileManager.default.temporaryDirectory.path
        let badge = LinkTargetResolver.presentationBadge(
            urls: nil,
            filePaths: nil,
            markdownLinks: [
                MarkdownRenderedLink(
                    range: NSRange(location: 0, length: 6),
                    target: directoryPath,
                    badge: .file
                )
            ],
            resolveFilePathDirectories: true
        )

        #expect(badge == .directory)
    }

    @Test func keepsMarkdownDirectoryLinksCheapOnRenderPath() {
        let directoryPath = FileManager.default.temporaryDirectory.path
        let badge = LinkTargetResolver.presentationBadge(
            urls: nil,
            filePaths: nil,
            markdownLinks: [
                MarkdownRenderedLink(
                    range: NSRange(location: 0, length: 6),
                    target: directoryPath,
                    badge: .file
                )
            ],
            resolveFilePathDirectories: false
        )

        #expect(badge == .file)
    }

    @Test func derivesMixedBadgeForMultipleTargetTypes() {
        let badge = LinkTargetResolver.presentationBadge(
            urls: ["https://example.com"],
            filePaths: ["/tmp/example.txt"],
            markdownLinks: []
        )

        #expect(badge == .links)
    }

    @Test func presentsPlainTextRowsWithNeutralTextIcon() {
        let presentation = MetadataBadgePresentation.make(kind: .plainText, linkBadge: nil)

        #expect(presentation.title == "plain_text")
        #expect(presentation.symbol == "text.alignleft")
        #expect(presentation.tintRole == .neutral)
    }

    @Test func presentsWebLinkRowsWithURLIcon() {
        let presentation = MetadataBadgePresentation.make(kind: .plainText, linkBadge: .url)

        #expect(presentation.title == "url")
        #expect(presentation.symbol == "link")
        #expect(presentation.tintRole == .url)
    }

    @Test func presentsFileRowsWithDocumentIcon() {
        let presentation = MetadataBadgePresentation.make(kind: .plainText, linkBadge: .file)

        #expect(presentation.title == "file")
        #expect(presentation.symbol == "doc")
        #expect(presentation.tintRole == .file)
    }

    @Test func presentsDirectoryRowsWithFolderIcon() {
        let presentation = MetadataBadgePresentation.make(kind: .plainText, linkBadge: .directory)

        #expect(presentation.title == "directory")
        #expect(presentation.symbol == "folder")
        #expect(presentation.tintRole == .directory)
    }

    @Test func presentsMixedLinkRowsWithMixedLinkIcon() {
        let presentation = MetadataBadgePresentation.make(kind: .plainText, linkBadge: .links)

        #expect(presentation.title == "links")
        #expect(presentation.symbol == "link.badge.plus")
        #expect(presentation.tintRole == .links)
    }

    @Test func hitTestingFindsActionableWebTargetsInsideLinkRange() throws {
        let target = try #require(actionableTarget(target: "https://example.com", point: NSPoint(x: 6, y: 6)))

        guard case .web(let url) = target else {
            Issue.record("Expected web target")
            return
        }
        #expect(url.absoluteString == "https://example.com")
    }

    @Test func hitTestingFindsActionableFileTargetsInsideLinkRange() throws {
        let target = try #require(actionableTarget(target: "/tmp/example.txt", point: NSPoint(x: 6, y: 6)))

        guard case .file(let url, let isDirectory) = target else {
            Issue.record("Expected file target")
            return
        }
        #expect(url.path == "/tmp/example.txt")
        #expect(isDirectory == false)
    }

    @Test func hitTestingIgnoresPointsOutsideLinkRange() {
        let target = actionableTarget(target: "https://example.com", point: NSPoint(x: 160, y: 6))

        #expect(target == nil)
    }

    @Test func hitTestingIgnoresTrailingEmptySpaceAfterLinkedText() {
        let target = MarkdownLinkHitTesting.actionableTarget(
            at: NSPoint(x: 160, y: 6),
            in: NSSize(width: 220, height: 30),
            attributedString: NSAttributedString(string: "link"),
            links: [
                MarkdownRenderedLink(
                    range: NSRange(location: 0, length: 4),
                    target: "https://example.com",
                    badge: .url
                )
            ],
            lineLimit: 1,
            lineBreakMode: .byTruncatingTail
        )

        #expect(target == nil)
    }

    @Test func hitTestingIgnoresUnsupportedTargetsInsideLinkRange() {
        let target = actionableTarget(target: "mailto:test@example.com", point: NSPoint(x: 6, y: 6))

        #expect(target == nil)
    }

    private func actionableTarget(target: String, point: NSPoint) -> LinkTargetResolution? {
        MarkdownLinkHitTesting.actionableTarget(
            at: point,
            in: NSSize(width: 220, height: 30),
            attributedString: NSAttributedString(string: "link target"),
            links: [
                MarkdownRenderedLink(
                    range: NSRange(location: 0, length: 4),
                    target: target,
                    badge: LinkTargetResolver.classify(target).badge
                )
            ],
            lineLimit: 1,
            lineBreakMode: .byTruncatingTail
        )
    }
}
