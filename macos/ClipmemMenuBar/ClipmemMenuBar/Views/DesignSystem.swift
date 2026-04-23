import SwiftUI

// MARK: - Spacing Tokens

enum Spacing {
    static let xxs: CGFloat = 2
    static let xs: CGFloat = 4
    static let sm: CGFloat = 8
    static let md: CGFloat = 12
    static let lg: CGFloat = 16
    static let xl: CGFloat = 24
    static let xxl: CGFloat = 32
}

// MARK: - Typography Scale

enum DesignType {
    static let windowTitle: Font = .title3.weight(.semibold)
    static let sectionHeader: Font = .headline
    static let bodyPrimary: Font = .body
    static let bodySecondary: Font = .callout
    static let rowTitle: Font = .body.weight(.medium)
    static let rowMeta: Font = .caption
    static let badge: Font = .caption2.weight(.bold)
    static let mono: Font = .body.monospaced()
    static let monoSmall: Font = .caption.monospacedDigit()
}

// MARK: - Corner Radius Scale

enum DesignRadius {
    static let sm: CGFloat = 6
    static let md: CGFloat = 8
    static let lg: CGFloat = 12
    static let xl: CGFloat = 16
}

// MARK: - Color System

enum DesignColor {
    static let surfaceGrouped = Color(.controlBackgroundColor)

    static func bannerBackground(_ tint: Color) -> Color {
        tint.opacity(0.10)
    }

    static func kindTint(for kind: String?) -> Color {
        switch kind {
        case "image": .purple
        case "pdf": .red
        case "url": .blue
        case "file": .yellow
        case "html": .teal
        case "rtf": .indigo
        case "binary": .brown
        default: .gray
        }
    }

    static func scoreTint(for score: Double) -> Color {
        if score >= 0.8 { return .green }
        if score >= 0.4 { return .orange }
        return .secondary
    }
}

// MARK: - Shadow Scale

enum DesignShadow {
    static let subtleColor = Color.black.opacity(0.06)
    static let subtleRadius: CGFloat = 2
    static let subtleY: CGFloat = 1

    static let mediumColor = Color.black.opacity(0.10)
    static let mediumRadius: CGFloat = 8
    static let mediumY: CGFloat = 4
}

// MARK: - Icon Sizing

enum DesignIcon {
    static let small: CGFloat = 14
    static let medium: CGFloat = 18
    static let large: CGFloat = 24
    static let hero: CGFloat = 48
}

// MARK: - Animation Constants

enum DesignAnimation {
    static let standard = Animation.spring(duration: 0.35, bounce: 0.15)
    static let quick = Animation.spring(duration: 0.25, bounce: 0.1)
    static let entrance = Animation.spring(duration: 0.4, bounce: 0.2)
    static let exit = Animation.easeOut(duration: 0.2)
}

// MARK: - View Modifiers

struct CardStyle: ViewModifier {
    @State private var isHovered = false

    func body(content: Content) -> some View {
        content
            .background(DesignColor.surfaceGrouped, in: .rect(cornerRadius: DesignRadius.md))
            .shadow(
                color: isHovered ? DesignShadow.subtleColor : .clear,
                radius: DesignShadow.subtleRadius,
                y: DesignShadow.subtleY
            )
            .onHover { isHovered = $0 }
            .animation(DesignAnimation.quick, value: isHovered)
    }
}

struct BannerStyle: ViewModifier {
    let tint: Color

    func body(content: Content) -> some View {
        content
            .padding(Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(DesignColor.bannerBackground(tint), in: .rect(cornerRadius: DesignRadius.md))
            .overlay(alignment: .leading) {
                tint
                    .frame(width: 3)
                    .clipShape(.rect(cornerRadii: .init(topLeading: DesignRadius.md, bottomLeading: DesignRadius.md)))
            }
    }
}

struct RowHighlightStyle: ViewModifier {
    let selected: Bool
    @State private var isHovered = false

    func body(content: Content) -> some View {
        content
            .padding(.vertical, Spacing.sm)
            .padding(.horizontal, Spacing.sm)
            .background(
                selected
                    ? Color.accentColor
                    : (isHovered ? Color.primary.opacity(0.04) : Color.clear),
                in: .rect(cornerRadius: DesignRadius.md)
            )
            .contentShape(Rectangle())
            .onHover { isHovered = $0 }
            .animation(DesignAnimation.quick, value: isHovered)
    }
}

struct GlassOverlay: ViewModifier {
    var cornerRadius: CGFloat = DesignRadius.lg

    func body(content: Content) -> some View {
        content
            .background(.ultraThinMaterial, in: .rect(cornerRadius: cornerRadius))
            .overlay {
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(.primary.opacity(0.08), lineWidth: 0.5)
            }
    }
}

// MARK: - View Extensions

extension View {
    func cardStyle() -> some View {
        modifier(CardStyle())
    }

    func bannerStyle(tint: Color) -> some View {
        modifier(BannerStyle(tint: tint))
    }

    func rowHighlightStyle(selected: Bool) -> some View {
        modifier(RowHighlightStyle(selected: selected))
    }

    func glassOverlay(cornerRadius: CGFloat = DesignRadius.lg) -> some View {
        modifier(GlassOverlay(cornerRadius: cornerRadius))
    }
}
