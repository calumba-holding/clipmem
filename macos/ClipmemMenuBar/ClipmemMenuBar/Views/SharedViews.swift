import SwiftUI

struct StatusBadge: View {
    let state: HealthState

    var body: some View {
        Label(state.title, systemImage: state.symbol)
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

    var body: some View {
        Label(message, systemImage: "exclamationmark.triangle")
            .font(.callout)
            .foregroundStyle(.red)
            .lineLimit(3)
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.red.opacity(0.08), in: .rect(cornerRadius: 8))
    }
}

struct FieldRow: View {
    let title: String
    let value: String?

    var body: some View {
        if let value, value.isEmpty == false {
            GridRow {
                Text(title)
                    .foregroundStyle(.secondary)
                    .gridColumnAlignment(.trailing)
                Text(value)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

struct FilterBar: View {
    @Bindable var history: HistoryModel

    var body: some View {
        HStack(spacing: 10) {
            Stepper("Hours: \(history.filters.hours)", value: $history.filters.hours, in: 1...720)
                .frame(width: 130)
            TextField("App", text: $history.filters.appName)
                .textFieldStyle(.roundedBorder)
                .frame(width: 140)
            TextField("Bundle ID", text: $history.filters.bundleID)
                .textFieldStyle(.roundedBorder)
                .frame(width: 180)
            Picker("Kind", selection: $history.filters.kind) {
                Text("Any").tag(ClipboardKind?.none)
                ForEach(ClipboardKind.allCases) { kind in
                    Text(kind.title).tag(Optional(kind))
                }
            }
            .frame(width: 130)
            Toggle("Text", isOn: $history.filters.hasText)
            Toggle("URL", isOn: $history.filters.hasURL)
            Toggle("File", isOn: $history.filters.hasFile)
            Toggle("Image", isOn: $history.filters.hasImage)
            Toggle("PDF", isOn: $history.filters.hasPDF)
        }
        .toggleStyle(.checkbox)
        .font(.callout)
    }
}
