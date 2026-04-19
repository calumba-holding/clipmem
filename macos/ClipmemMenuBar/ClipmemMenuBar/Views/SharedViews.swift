import SwiftUI
import UniformTypeIdentifiers

// MARK: - Design Constants

enum Spacing {
    static let xs: CGFloat = 4
    static let sm: CGFloat = 8
    static let md: CGFloat = 12
    static let lg: CGFloat = 16
    static let xl: CGFloat = 24
}

// MARK: - Utilities

func humanReadableType(_ uti: String) -> String {
    if let utType = UTType(uti), let desc = utType.localizedDescription {
        return desc
    }
    return uti
}

// MARK: - Shared Components

struct StatusBadge: View {
    let state: HealthState

    var body: some View {
        Label(state.title, systemImage: state.symbol)
            .symbolVariant(.fill)
            .foregroundStyle(state.tint)
            .font(.headline)
            .labelStyle(.titleAndIcon)
            .accessibilityLabel(state.title)
    }
}

struct EmptyStateView: View {
    let title: String
    let detail: String
    let symbol: String

    var body: some View {
        ContentUnavailableView {
            Label(title, systemImage: symbol)
        } description: {
            Text(detail)
        }
    }
}

struct ErrorBanner: View {
    let message: String
    var recovery: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.xs) {
            Label(message, systemImage: "exclamationmark.triangle")
                .font(.callout)
                .foregroundStyle(.red)
                .lineLimit(3)
            if let recovery {
                Text(recovery)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
        .padding(Spacing.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.red.opacity(0.08), in: .rect(cornerRadius: Spacing.sm))
    }
}

struct ActionFeedbackOverlay: View {
    let message: String?

    var body: some View {
        if let message {
            Text(message)
                .font(.callout.weight(.medium))
                .padding(.horizontal, Spacing.lg)
                .padding(.vertical, Spacing.sm)
                .background(.regularMaterial, in: Capsule())
                .transition(.move(edge: .top).combined(with: .opacity))
        }
    }
}

struct FieldRow: View {
    let title: String
    let value: String?
    var lineLimit: Int = 2
    var showPlaceholder: Bool = false

    var body: some View {
        if let value, value.isEmpty == false {
            GridRow {
                Text(title)
                    .foregroundStyle(.secondary)
                    .gridColumnAlignment(.trailing)
                Text(value)
                    .textSelection(.enabled)
                    .lineLimit(lineLimit)
                    .truncationMode(.middle)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help(value)
            }
        } else if showPlaceholder {
            GridRow {
                Text(title)
                    .foregroundStyle(.secondary)
                    .gridColumnAlignment(.trailing)
                Text("—")
                    .foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

struct FilterBar: View {
    @Bindable var history: HistoryModel
    @State private var showAdvanced = false

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.sm) {
            HStack(spacing: Spacing.md) {
                Stepper("Hours: \(history.filters.hours)", value: $history.filters.hours, in: 1...720)
                    .fixedSize()
                Picker("Kind", selection: $history.filters.kind) {
                    Text("Any").tag(ClipboardKind?.none)
                    ForEach(ClipboardKind.allCases) { kind in
                        Text(kind.title).tag(Optional(kind))
                    }
                }
                .fixedSize()
                Button {
                    withAnimation { showAdvanced.toggle() }
                } label: {
                    HStack(spacing: Spacing.xs) {
                        Label("Filters", systemImage: showAdvanced ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                        if history.filters.activeAdvancedFilterCount > 0 {
                            Text("\(history.filters.activeAdvancedFilterCount)")
                                .font(.caption2.weight(.bold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 5)
                                .padding(.vertical, 1)
                                .background(.blue, in: Capsule())
                        }
                    }
                }
                .buttonStyle(.borderless)
            }
            if showAdvanced {
                HStack(spacing: Spacing.md) {
                    TextField("Application name", text: $history.filters.appName)
                        .textFieldStyle(.roundedBorder)
                        .frame(minWidth: 100)
                    TextField("App identifier", text: $history.filters.bundleID)
                        .textFieldStyle(.roundedBorder)
                        .frame(minWidth: 100)
                }
                HStack(spacing: Spacing.md) {
                    Toggle("Text", isOn: $history.filters.hasText)
                    Toggle("URL", isOn: $history.filters.hasURL)
                    Toggle("File", isOn: $history.filters.hasFile)
                    Toggle("Image", isOn: $history.filters.hasImage)
                    Toggle("PDF", isOn: $history.filters.hasPDF)
                    Spacer()
                    if history.filters.activeAdvancedFilterCount > 0 {
                        Button("Reset") {
                            history.filters.resetAdvanced()
                        }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.secondary)
                    }
                }
            }
        }
        .toggleStyle(.checkbox)
        .font(.callout)
    }
}
