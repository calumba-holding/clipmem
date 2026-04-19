import SwiftUI

struct ResultRowView: View {
    let item: ClipmemItem
    let selected: Bool

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: symbol)
                .foregroundStyle(selected ? .white : .secondary)
                .frame(width: 18)
            VStack(alignment: .leading, spacing: 4) {
                Text(item.displayText)
                    .lineLimit(2)
                    .truncationMode(.tail)
                    .font(.body)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text(metadata)
                .font(.caption)
                .foregroundStyle(selected ? .white.opacity(0.82) : .secondary)
                .lineLimit(1)
                .truncationMode(.tail)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Spacer()
            if let score = item.score {
                Text(score.formatted(.number.precision(.fractionLength(2))))
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(selected ? .white.opacity(0.82) : .secondary)
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 6)
        .background(selected ? Color.accentColor : Color.clear, in: .rect(cornerRadius: 6))
        .contentShape(Rectangle())
    }

    private var metadata: String {
        [
            item.kind ?? "unknown",
            item.observedAt,
            item.appHint,
            item.whyMatched?.isEmpty == false ? item.whyMatched : nil,
        ]
        .compactMap { $0 }
        .joined(separator: "   ")
    }

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
