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

enum DisplayFormatters {
    static func localTimestamp(
        _ value: String?,
        timeZone: TimeZone = .current,
        locale: Locale = .current
    ) -> String? {
        guard let date = parseTimestamp(value) else { return nil }
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.timeZone = timeZone
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    static func byteCount(_ bytes: Int?) -> String? {
        guard let bytes else { return nil }
        return ByteCountFormatStyle(style: .file).format(Int64(bytes))
    }

    private static func parseTimestamp(_ value: String?) -> Date? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines), value.isEmpty == false else {
            return nil
        }

        if let date = iso8601Date(from: value) {
            return date
        }

        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return formatter.date(from: value)
    }

    private static func iso8601Date(from value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: value) {
            return date
        }

        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: value)
    }
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

struct HealthBanner: View {
    let state: HealthState
    var errorDetail: UserError? = nil
    var isRunningAction = false
    var actionLabel: String? = nil
    var onAction: (() -> Void)? = nil

    var body: some View {
        if state != .healthy && state != .unknown {
            HStack(spacing: Spacing.sm) {
                Image(systemName: state.symbol)
                    .foregroundStyle(state.tint)
                VStack(alignment: .leading, spacing: 2) {
                    Text(errorDetail?.message ?? state.title)
                        .font(.callout.weight(.medium))
                        .lineLimit(2)
                    if let recovery = errorDetail?.recovery ?? state.recoveryGuidance {
                        Text(recovery)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
                Spacer()
                if let actionLabel, let onAction {
                    Button(actionLabel, action: onAction)
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .disabled(isRunningAction)
                }
            }
            .padding(Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(state.tint.opacity(0.08), in: .rect(cornerRadius: Spacing.sm))
            .transition(.move(edge: .top).combined(with: .opacity))
            .accessibilityElement(children: .combine)
            .accessibilityLabel("\(state.title). \(state.recoveryGuidance ?? "")")
        }
    }
}

struct UpdateBanner: View {
    let status: UpdateStatus
    var onCopyCommand: (() -> Void)? = nil
    var onOpenRelease: (() -> Void)? = nil

    var body: some View {
        if status.isUpdateAvailable {
            HStack(spacing: Spacing.sm) {
                Image(systemName: "arrow.down.circle.fill")
                    .foregroundStyle(.blue)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Update available \u{2014} v\(status.latestVersion ?? "")")
                        .font(.callout.weight(.medium))
                        .lineLimit(1)
                    Text("You have v\(status.currentVersion)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if status.shouldShowHomebrewCommand, let onCopyCommand {
                    Button("Copy Command", action: onCopyCommand)
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                } else if let onOpenRelease {
                    Button("Open Release", action: onOpenRelease)
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .disabled(status.releaseURL == nil)
                }
            }
            .padding(Spacing.md)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.blue.opacity(0.08), in: .rect(cornerRadius: Spacing.sm))
            .transition(.move(edge: .top).combined(with: .opacity))
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
