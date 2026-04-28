import SwiftUI

enum MarkdownDisplayStyle {
    case compactRow
    case detail
}

enum MarkdownTextRenderer {
    static func render(_ source: String, style: MarkdownDisplayStyle) -> AttributedString {
        let lines = source.split(separator: "\n", omittingEmptySubsequences: false)
        guard lines.isEmpty == false else { return AttributedString(source) }

        var rendered = AttributedString()
        for (index, line) in lines.enumerated() {
            if index > 0 {
                rendered += AttributedString("\n")
            }

            let text = String(line)
            if let heading = heading(in: text) {
                var renderedLine = renderInline(heading.text, style: style)
                renderedLine.font = headingFont(level: heading.level, style: style)
                rendered += renderedLine
            } else {
                rendered += renderInline(text, style: style)
            }
        }

        return rendered
    }

    private static func renderInline(_ source: String, style: MarkdownDisplayStyle) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        guard var rendered = try? AttributedString(markdown: source, options: options) else {
            return AttributedString(source)
        }

        styleInlinePresentation(in: &rendered, style: style)
        styleLinks(in: &rendered)
        return rendered
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
}
