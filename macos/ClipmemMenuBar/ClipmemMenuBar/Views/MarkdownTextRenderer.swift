import SwiftUI

enum MarkdownDisplayStyle {
    case compactRow
    case detail
}

struct MarkdownRenderedText {
    var attributed: AttributedString
    var links: [MarkdownRenderedLink]

    var visibleText: String {
        String(attributed.characters)
    }
}

struct MarkdownRenderedLink: Equatable {
    var range: NSRange
    var target: String
    var badge: LinkPresentationBadge?
}

enum MarkdownTextRenderer {
    static func render(_ source: String, style: MarkdownDisplayStyle) -> AttributedString {
        renderedText(source, style: style).attributed
    }

    static func renderedText(_ source: String, style: MarkdownDisplayStyle) -> MarkdownRenderedText {
        let lines = source.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.isEmpty == false else {
            return MarkdownRenderedText(attributed: AttributedString(source), links: [])
        }

        var rendered = AttributedString()
        var links: [MarkdownRenderedLink] = []
        var utf16Offset = 0

        for (index, line) in lines.enumerated() {
            if index > 0 {
                rendered += AttributedString("\n")
                utf16Offset += 1
            }

            let text = String(line)
            let lineResult: InlineRenderResult
            if let heading = heading(in: text) {
                var renderedLine = renderInline(heading.text, style: style)
                renderedLine.attributed.font = headingFont(level: heading.level, style: style)
                lineResult = renderedLine
            } else {
                lineResult = renderInline(text, style: style)
            }

            rendered += lineResult.attributed
            links += lineResult.links.map { link in
                MarkdownRenderedLink(
                    range: NSRange(location: link.range.location + utf16Offset, length: link.range.length),
                    target: link.target,
                    badge: link.badge
                )
            }
            utf16Offset += lineResult.visibleUTF16Length
        }

        return MarkdownRenderedText(attributed: rendered, links: links)
    }

    private static func renderInline(_ source: String, style: MarkdownDisplayStyle) -> InlineRenderResult {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        guard var rendered = try? AttributedString(markdown: source, options: options) else {
            return InlineRenderResult(attributed: AttributedString(source), links: [])
        }

        let links = linkRuns(in: rendered)
        styleInlinePresentation(in: &rendered, style: style)
        styleLinks(in: &rendered)
        return InlineRenderResult(attributed: rendered, links: links)
    }

    private static func linkRuns(in attributedString: AttributedString) -> [MarkdownRenderedLink] {
        attributedString.runs.compactMap { run in
            guard let link = run.link else { return nil }
            let lowerBound = String(attributedString.characters[attributedString.startIndex..<run.range.lowerBound])
            let linkedText = String(attributedString.characters[run.range])
            let range = NSRange(location: lowerBound.utf16.count, length: linkedText.utf16.count)
            let target = link.absoluteString
            return MarkdownRenderedLink(
                range: range,
                target: target,
                badge: LinkTargetResolver.classify(target).badge
            )
        }
    }

    private static func styleInlinePresentation(in attributedString: inout AttributedString, style: MarkdownDisplayStyle) {
        for run in attributedString.runs {
            let intent = run.inlinePresentationIntent
            if intent?.contains(.stronglyEmphasized) == true {
                attributedString[run.range].font = strongFont(style: style)
            } else if intent?.contains(.emphasized) == true {
                attributedString[run.range].font = italicFont(style: style)
            } else {
                attributedString[run.range].font = regularFont(style: style)
            }
        }
    }

    private static func styleLinks(in attributedString: inout AttributedString) {
        for run in attributedString.runs {
            guard run.link != nil else { continue }
            attributedString[run.range].link = nil
            attributedString[run.range].foregroundColor = .accentColor
            attributedString[run.range].underlineStyle = Text.LineStyle(pattern: .solid, color: .accentColor)
        }
    }

    private static func heading(in line: String) -> (level: Int, text: String)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.first == "#" else { return nil }

        var level = 0
        var index = trimmed.startIndex
        while index < trimmed.endIndex, trimmed[index] == "#", level < 6 {
            level += 1
            index = trimmed.index(after: index)
        }

        guard level > 0, index < trimmed.endIndex, trimmed[index].isWhitespace else { return nil }
        let textStart = trimmed.index(after: index)
        let text = String(trimmed[textStart...]).trimmingCharacters(in: .whitespaces)
        return text.isEmpty ? nil : (level, text)
    }

    private static func headingFont(level: Int, style: MarkdownDisplayStyle) -> Font {
        switch style {
        case .compactRow:
            return DesignType.rowTitle.weight(.semibold)
        case .detail:
            switch level {
            case 1:
                return .title3.weight(.semibold)
            case 2:
                return .headline
            default:
                return .subheadline.weight(.semibold)
            }
        }
    }

    private static func regularFont(style: MarkdownDisplayStyle) -> Font {
        switch style {
        case .compactRow:
            return .callout
        case .detail:
            return DesignType.bodyPrimary
        }
    }

    private static func italicFont(style: MarkdownDisplayStyle) -> Font {
        switch style {
        case .compactRow:
            return .callout.italic()
        case .detail:
            return DesignType.bodyPrimary.italic()
        }
    }

    private static func strongFont(style: MarkdownDisplayStyle) -> Font {
        switch style {
        case .compactRow:
            return .body.weight(.bold)
        case .detail:
            return DesignType.bodyPrimary.weight(.bold)
        }
    }

    private struct InlineRenderResult {
        var attributed: AttributedString
        var links: [MarkdownRenderedLink]

        var visibleUTF16Length: Int {
            String(attributed.characters).utf16.count
        }
    }
}
