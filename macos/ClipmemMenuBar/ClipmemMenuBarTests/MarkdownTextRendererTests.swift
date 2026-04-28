import Foundation
import SwiftUI
import Testing
@testable import ClipmemMenuBar

struct MarkdownTextRendererTests {
    @Test func rendersBoldWithoutDelimiters() throws {
        let rendered = MarkdownTextRenderer.render("make **this** bold", style: .detail)

        #expect(String(rendered.characters) == "make this bold")
        let run = try #require(Self.run(containing: "this", in: rendered))
        #expect(run.inlinePresentationIntent?.contains(InlinePresentationIntent.stronglyEmphasized) == true)
        #expect(run.font == DesignType.bodyPrimary.weight(.bold))
    }

    @Test func rendersItalicWithoutDelimiters() throws {
        let rendered = MarkdownTextRenderer.render("make *this* italic", style: .detail)

        #expect(String(rendered.characters) == "make this italic")
        let run = try #require(Self.run(containing: "this", in: rendered))
        #expect(run.inlinePresentationIntent?.contains(InlinePresentationIntent.emphasized) == true)
        #expect(run.font == DesignType.bodyPrimary.italic())
    }

    @Test func makesCompactBoldMoreDistinctThanRegularText() throws {
        let rendered = MarkdownTextRenderer.render("plain **bold**", style: .compactRow)

        let plainRun = try #require(Self.run(containing: "plain", in: rendered))
        let boldRun = try #require(Self.run(containing: "bold", in: rendered))
        #expect(plainRun.font == Font.callout)
        #expect(boldRun.font == Font.body.weight(.bold))
    }

    @Test func rendersHeadingsAsStyledPlainTitleLines() throws {
        let rendered = MarkdownTextRenderer.render("# Release Notes\nBody", style: .detail)

        #expect(String(rendered.characters) == "Release Notes\nBody")
        let run = try #require(Self.run(containing: "Release Notes", in: rendered))
        #expect(run.font != nil)
    }

    @Test func rendersIndentedMarkdownHeadingsThroughThreeSpaces() {
        let rendered = MarkdownTextRenderer.render("   # Release Notes", style: .detail)

        #expect(String(rendered.characters) == "Release Notes")
    }

    @Test func preservesHeavilyIndentedHeadingLikeText() {
        let source = "    # comment"
        let rendered = MarkdownTextRenderer.render(source, style: .detail)

        #expect(String(rendered.characters) == source)
    }

    @Test func rendersLinksAsStyledTextWithActionableMetadata() throws {
        let result = MarkdownTextRenderer.renderedText("[Clipmem](https://example.com)", style: .detail)
        let rendered = result.attributed

        #expect(String(rendered.characters) == "Clipmem")
        let run = try #require(Self.run(containing: "Clipmem", in: rendered))
        #expect(run.link == nil)
        #expect(run.underlineStyle != nil)
        #expect(run.foregroundColor != nil)

        let link = try #require(result.links.first)
        #expect(result.links.count == 1)
        #expect(link.range.location == 0)
        #expect(link.range.length == "Clipmem".utf16.count)
        #expect(link.target == "https://example.com")
        #expect(link.badge == .url)
    }

    @Test func rendersLinksWithCustomForegroundColor() throws {
        let result = MarkdownTextRenderer.renderedText(
            "[Clipmem](https://example.com)",
            style: .compactRow,
            linkColor: .white
        )

        let run = try #require(Self.run(containing: "Clipmem", in: result.attributed))
        #expect(run.foregroundColor == .white)
        #expect(run.underlineStyle != nil)
    }

    @Test func rendersFileLinksWithFileMetadata() throws {
        let result = MarkdownTextRenderer.renderedText("[local](file:///tmp/example.txt)", style: .detail)

        #expect(result.visibleText == "local")
        let link = try #require(result.links.first)
        #expect(result.links.count == 1)
        #expect(link.range.location == 0)
        #expect(link.range.length == "local".utf16.count)
        #expect(link.target == "file:///tmp/example.txt")
        #expect(link.badge == .file)
    }

    @Test func rendersAbsolutePathLinksWithFileMetadata() throws {
        let result = MarkdownTextRenderer.renderedText("[local](/tmp/example.txt)", style: .detail)

        #expect(result.visibleText == "local")
        let link = try #require(result.links.first)
        #expect(link.target == "/tmp/example.txt")
        #expect(link.badge == .file)
    }

    @Test func keepsRelativeLinksVisuallyStyledButNonActionable() throws {
        let result = MarkdownTextRenderer.renderedText("[relative](docs/file.md)", style: .detail)

        #expect(result.visibleText == "relative")
        let link = try #require(result.links.first)
        #expect(link.target == "docs/file.md")
        #expect(link.badge == nil)
    }

    @Test func preservesPlainText() {
        let source = "plain text with # not-a-heading and brackets [ok]"

        let result = MarkdownTextRenderer.renderedText(source, style: .detail)

        #expect(result.visibleText == source)
        #expect(result.links.isEmpty)
    }

    @Test func keepsEmptyAndWhitespaceOnlyTextReadable() {
        #expect(String(MarkdownTextRenderer.render("", style: .detail).characters) == "")
        #expect(String(MarkdownTextRenderer.render("   ", style: .detail).characters) == "   ")
    }

    @Test func malformedMarkdownFallsBackToReadableText() {
        let result = MarkdownTextRenderer.renderedText("bad [link](", style: .detail)

        #expect(result.visibleText == "bad [link](")
        #expect(result.links.isEmpty)
    }

    private static func run(containing needle: String, in rendered: AttributedString) -> AttributedString.Runs.Run? {
        rendered.runs.first { run in
            String(rendered.characters[run.range]).contains(needle)
        }
    }
}
