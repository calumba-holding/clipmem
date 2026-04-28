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

    @Test func rendersLinksAsNonClickableStyledText() throws {
        let rendered = MarkdownTextRenderer.render("[Clipmem](https://example.com)", style: .detail)

        #expect(String(rendered.characters) == "Clipmem")
        let run = try #require(Self.run(containing: "Clipmem", in: rendered))
        #expect(run.link == nil)
        #expect(run.underlineStyle != nil)
        #expect(run.foregroundColor != nil)
    }

    @Test func preservesPlainText() {
        let source = "plain text with # not-a-heading and brackets [ok]"

        let rendered = MarkdownTextRenderer.render(source, style: .detail)

        #expect(String(rendered.characters) == source)
    }

    @Test func keepsEmptyAndWhitespaceOnlyTextReadable() {
        #expect(String(MarkdownTextRenderer.render("", style: .detail).characters) == "")
        #expect(String(MarkdownTextRenderer.render("   ", style: .detail).characters) == "   ")
    }

    @Test func malformedMarkdownFallsBackToReadableText() {
        let rendered = MarkdownTextRenderer.render("bad [link](", style: .detail)

        #expect(String(rendered.characters) == "bad [link](")
    }

    private static func run(containing needle: String, in rendered: AttributedString) -> AttributedString.Runs.Run? {
        rendered.runs.first { run in
            String(rendered.characters[run.range]).contains(needle)
        }
    }
}
