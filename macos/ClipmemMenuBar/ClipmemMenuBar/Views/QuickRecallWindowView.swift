import SwiftUI

struct QuickRecallWindowView: View {
    let appModel: AppModel

    @Environment(\.dismiss) private var dismiss
    @Environment(\.openWindow) private var openWindow
    @FocusState private var queryFocused: Bool
    @State private var quick: QuickRecallModel
    @State private var confirmForget = false
    @State private var pendingForgetItem: ClipmemItem?

    init(appModel: AppModel) {
        self.appModel = appModel
        _quick = State(initialValue: QuickRecallModel(appModel: appModel))
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            list
            if let error = quick.error {
                ErrorBanner(message: error.message, recovery: error.recovery)
                    .padding()
            }
            Divider()
            footer
        }
        .overlay(alignment: .top) {
            ActionFeedbackOverlay(message: appModel.actionMessage)
                .padding(.top, Spacing.sm)
        }
        .task {
            queryFocused = true
            await quick.refresh()
        }
        .onMoveCommand { direction in
            switch direction {
            case .down: quick.moveSelection(1)
            case .up: quick.moveSelection(-1)
            default: break
            }
        }
        .onExitCommand {
            dismiss()
        }
        .onKeyPress(.space) {
            guard quick.selectedItem != nil, queryFocused == false else { return .ignored }
            openHistory()
            return .handled
        }
        .confirmationDialog("Forget this snapshot?", isPresented: $confirmForget) {
            Button("Forget", role: .destructive) {
                let item = pendingForgetItem
                Task {
                    if let item {
                        await quick.forget(item)
                    }
                    pendingForgetItem = nil
                }
            }
            Button("Cancel", role: .cancel) {
                pendingForgetItem = nil
            }
        } message: {
            Text("This permanently removes the saved content and all records of when it was copied. This cannot be undone.")
        }
        .onChange(of: confirmForget) {
            if confirmForget == false {
                pendingForgetItem = nil
            }
        }
    }

    private var header: some View {
        HStack(spacing: Spacing.md) {
            Picker("Mode", selection: $quick.mode) {
                ForEach([QueryMode.recall, .search, .recent, .timeline]) { mode in
                    Text(mode.title).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .fixedSize()
            .onChange(of: quick.mode) {
                Task { await quick.refresh() }
            }

            TextField("Search clipboard history", text: $quick.query)
                .textFieldStyle(.roundedBorder)
                .focused($queryFocused)
                .onSubmit {
                    Task { await quick.restoreSelected() }
                }
                .onChange(of: quick.query) {
                    quick.queryChanged()
                }
        }
        .padding()
    }

    private var list: some View {
        List(selection: $quick.selectedID) {
            ForEach(quick.results) { item in
                ResultRowView(item: item, selected: item.snapshotId == quick.selectedID)
                    .tag(item.snapshotId)
                    .contextMenu {
                        Button("Restore") { Task { await appModel.restore(item) } }
                        Button("Open in History") { openHistory(item: item) }
                        Button("Forget", role: .destructive) {
                            pendingForgetItem = item
                            confirmForget = true
                        }
                    }
            }
        }
        .listStyle(.inset)
        .overlay {
            if quick.isLoading {
                ProgressView()
            } else if quick.results.isEmpty {
                EmptyStateView(title: "No Results", detail: "Try another query or switch modes.", symbol: "magnifyingglass")
            }
        }
    }

    private var footer: some View {
        HStack {
            Button("Restore", systemImage: "arrow.uturn.backward.square") {
                Task { await quick.restoreSelected() }
            }
            .keyboardShortcut(.return, modifiers: [])
            .disabled(quick.selectedItem == nil)
            .help("Restore to clipboard (Return)")
            Button("Open", systemImage: "rectangle.stack.badge.play") {
                openHistory()
            }
            .keyboardShortcut("o", modifiers: .command)
            .disabled(quick.selectedItem == nil)
            .help("Open in History (\u{2318}O)")
            Button("Forget", systemImage: "trash", role: .destructive) {
                pendingForgetItem = quick.selectedItem
                confirmForget = true
            }
            .keyboardShortcut(.delete, modifiers: [])
            .disabled(quick.selectedItem == nil)
            .help("Remove this item (Delete)")
            Spacer()
            Text("Space to preview")
                .font(.caption)
                .foregroundStyle(.tertiary)
            Button("Focus Search", systemImage: "magnifyingglass") {
                queryFocused = true
            }
            .keyboardShortcut("f", modifiers: .command)
            .help("Focus search field (\u{2318}F)")
            Button("Refresh", systemImage: "arrow.clockwise") {
                Task { await quick.refresh() }
            }
            .keyboardShortcut("r", modifiers: .command)
            .help("Refresh results (\u{2318}R)")
        }
        .padding()
    }

    private func openHistory() {
        guard let item = quick.selectedItem else { return }
        openHistory(item: item)
    }

    private func openHistory(item: ClipmemItem) {
        appModel.requestHistoryFocus(snapshotID: item.snapshotId, mode: quick.mode, query: quick.query)
        WindowActivation.openWindow(openWindow, id: .history)
    }
}
