import SwiftUI

struct ResultRowView: View {
    let item: ClipmemItem
    let selected: Bool
    var animatedHighlight = true

    var body: some View {
        let renderedText = MarkdownTextRenderer.renderedText(item.displayText, style: .compactRow)
        let badgePresentation = metadataBadgePresentation(renderedText: renderedText)

        HStack(alignment: .top, spacing: Spacing.md) {
            iconView(presentation: badgePresentation)
                .padding(.top, Spacing.xxs)
            VStack(alignment: .leading, spacing: Spacing.xs) {
                CommandClickableMarkdownText(
                    rendered: renderedText,
                    lineLimit: 2,
                    truncationMode: .tail
                )
                    .font(DesignType.rowTitle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help(item.displayText)
                metadataView(presentation: badgePresentation)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Spacer()
            scoreView
        }
        .rowHighlightStyle(selected: selected, animated: animatedHighlight)
    }

    // MARK: - Icon

    private func iconView(presentation: MetadataBadgePresentation) -> some View {
        Image(systemName: presentation.symbol)
            .font(.system(size: DesignIcon.small))
            .foregroundStyle(selected ? .white : metadataTint(for: presentation))
            .frame(width: 26, height: 26)
            .background(
                (selected ? Color.white.opacity(0.2) : metadataTint(for: presentation).opacity(0.12))
            )
            .clipShape(Circle())
    }

    private func metadataTint(for presentation: MetadataBadgePresentation) -> Color {
        DesignColor.metadataBadgeTint(for: presentation.tintRole)
    }

    // MARK: - Metadata

    private func metadataView(presentation: MetadataBadgePresentation) -> some View {
        HStack(spacing: Spacing.xs) {
            MetadataBadge(
                presentation: presentation,
                selected: selected
            )
            if let relative = DisplayFormatters.relativeTimestamp(item.observedAt) {
                Text(relative)
                    .font(DesignType.rowMeta)
                    .foregroundStyle(selected ? .white.opacity(0.82) : .secondary)
                    .help(DisplayFormatters.localTimestamp(item.observedAt) ?? item.observedAt ?? "")
            }
            if let app = item.appHint {
                Text("\u{00B7}")
                    .foregroundStyle(selected ? Color.white.opacity(0.5) : Color(.tertiaryLabelColor))
                Text(app)
                    .font(DesignType.rowMeta)
                    .foregroundStyle(selected ? .white.opacity(0.82) : .secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
    }

    private func metadataBadgePresentation(renderedText: MarkdownRenderedText) -> MetadataBadgePresentation {
        let linkBadge = LinkTargetResolver.presentationBadge(
            urls: item.urls,
            filePaths: item.filePaths,
            markdownLinks: renderedText.links
        )
        return MetadataBadgePresentation.make(kind: item.kind, linkBadge: linkBadge)
    }

    // MARK: - Score

    @ViewBuilder
    private var scoreView: some View {
        if let score = item.score {
            Text(score.formatted(.number.precision(.fractionLength(2))))
                .font(DesignType.monoSmall)
                .foregroundStyle(selected ? .white.opacity(0.82) : DesignColor.scoreTint(for: score))
        }
    }

    // MARK: - Symbol
}

private struct MetadataBadge: View {
    let presentation: MetadataBadgePresentation
    let selected: Bool

    var body: some View {
        Text(presentation.title)
            .font(DesignType.badge)
            .lineLimit(1)
            .padding(.horizontal, Spacing.xs)
            .padding(.vertical, Spacing.xxs)
            .background(background, in: Capsule())
            .foregroundStyle(foreground)
    }

    private var tint: Color {
        DesignColor.metadataBadgeTint(for: presentation.tintRole)
    }

    private var foreground: Color {
        selected ? .white : tint
    }

    private var background: Color {
        if selected {
            return Color.white.opacity(0.2)
        }
        if presentation.tintRole == .neutral {
            return Color(.quaternaryLabelColor)
        }
        return tint.opacity(0.12)
    }
}
