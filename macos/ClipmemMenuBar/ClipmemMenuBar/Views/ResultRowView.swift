import SwiftUI

struct ResultRowView: View {
    let item: ClipmemItem
    let selected: Bool

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Spacing.md) {
            iconView
            VStack(alignment: .leading, spacing: Spacing.xs) {
                Text(item.displayText)
                    .lineLimit(2)
                    .truncationMode(.tail)
                    .font(DesignType.rowTitle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help(item.displayText)
                metadataView
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Spacer()
            scoreView
        }
        .rowHighlightStyle(selected: selected)
    }

    // MARK: - Icon

    private var iconView: some View {
        Image(systemName: symbol)
            .font(.system(size: DesignIcon.small))
            .foregroundStyle(selected ? .white : kindTint)
            .frame(width: 26, height: 26)
            .background(
                (selected ? Color.white.opacity(0.2) : kindTint.opacity(0.12))
            )
            .clipShape(Circle())
    }

    private var kindTint: Color {
        DesignColor.kindTint(for: item.kind)
    }

    // MARK: - Metadata

    private var metadataView: some View {
        HStack(spacing: Spacing.xs) {
            if let kind = item.kind {
                Text(kind)
                    .font(DesignType.badge)
                    .padding(.horizontal, Spacing.xs)
                    .padding(.vertical, Spacing.xxs)
                    .background(
                        selected ? Color.white.opacity(0.2) : Color(.quaternaryLabelColor),
                        in: Capsule()
                    )
                    .foregroundStyle(selected ? .white : .secondary)
            }
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

    private var symbol: String {
        switch item.kind {
        case "image": "photo"
        case "pdf": "doc.richtext"
        case "url": "link"
        case "file": "doc"
        case "html": "chevron.left.forwardslash.chevron.right"
        case "rtf": "textformat"
        case "binary": "shippingbox"
        default: "text.alignleft"
        }
    }
}
