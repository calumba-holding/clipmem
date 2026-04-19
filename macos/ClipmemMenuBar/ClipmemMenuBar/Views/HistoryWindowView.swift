import SwiftUI

struct HistoryWindowView: View {
    let appModel: AppModel

    @Environment(\.openWindow) private var openWindow
    @State private var history: HistoryModel
    @SceneStorage("history.mode") private var storedMode = QueryMode.recent.rawValue
    @SceneStorage("history.query") private var storedQuery = ""
    @SceneStorage("history.inspector") private var inspectorPresented = true
    @SceneStorage("history.selected") private var storedSelectedID = 0

    init(appModel: AppModel) {
        self.appModel = appModel
        _history = State(initialValue: HistoryModel(appModel: appModel))
    }

    var body: some View {
        NavigationSplitView {
            List(selection: $history.mode) {
                Section("Browse") {
                    ForEach([QueryMode.recall, .search, .recent, .timeline]) { mode in
                        Label(mode.title, systemImage: mode.symbol)
                            .tag(mode)
                    }
                }
                Section("System") {
                    Label(QueryMode.diagnostics.title, systemImage: QueryMode.diagnostics.symbol)
                        .tag(QueryMode.diagnostics)
                }
            }
            .navigationTitle("clipmem")
        } detail: {
            content
                .navigationTitle(history.mode.title)
                .toolbar {
                    ToolbarItemGroup {
                        Button("Refresh", systemImage: "arrow.clockwise") {
                            Task { await history.reload() }
                        }
                        .keyboardShortcut("r", modifiers: .command)
                        Button("Quick Recall", systemImage: "bolt") {
                            openWindow(id: WindowID.quickRecall.rawValue)
                        }
                    }
                }
        }
        .inspector(isPresented: $inspectorPresented) {
            inspector
                .inspectorColumnWidth(min: 260, ideal: 300, max: 380)
        }
        .task {
            restoreSceneState()
            await history.reload()
        }
        .onChange(of: history.mode) {
            storedMode = history.mode.rawValue
            Task { await history.reload() }
        }
        .onChange(of: history.query) {
            storedQuery = history.query
        }
        .onChange(of: history.selectedID) {
            storedSelectedID = history.selectedID ?? 0
            Task { await history.loadSelectedDetail() }
        }
    }

    @ViewBuilder
    private var content: some View {
        if history.mode == .diagnostics {
            DiagnosticsView(appModel: appModel)
        } else {
            VStack(spacing: 0) {
                VStack(spacing: 10) {
                    HStack {
                        Picker("Mode", selection: $history.mode) {
                            ForEach([QueryMode.recall, .search, .recent, .timeline]) { mode in
                                Text(mode.title).tag(mode)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(width: 360)
                        TextField(searchPrompt, text: $history.query)
                            .textFieldStyle(.roundedBorder)
                            .disabled(history.mode == .recent || history.mode == .timeline)
                            .onSubmit {
                                Task { await history.reload() }
                            }
                        Button("Search", systemImage: "magnifyingglass") {
                            Task { await history.reload() }
                        }
                        .disabled((history.mode == .search || history.mode == .recall) && history.query.isEmpty && history.mode == .search)
                    }
                    FilterBar(history: history)
                }
                .padding()

                Divider()

                HSplitView {
                    resultList
                        .frame(minWidth: 360, idealWidth: 440)
                    SnapshotDetailView(detail: history.selectedDetail, fallback: history.selectedItem)
                        .frame(minWidth: 420)
                }
            }
        }
    }

    private var resultList: some View {
        VStack(spacing: 0) {
            if let error = history.errorMessage {
                ErrorBanner(message: error)
                    .padding()
            }
            List(selection: $history.selectedID) {
                ForEach(history.results) { item in
                    ResultRowView(item: item, selected: item.snapshotId == history.selectedID)
                        .tag(item.snapshotId)
                }
                if history.nextCursor != nil {
                    Button("Load More", systemImage: "arrow.down.circle") {
                        Task { await history.loadMore() }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 8)
                }
            }
            .listStyle(.inset)
            if history.isLoading {
                ProgressView()
                    .padding(8)
            }
        }
    }

    private var inspector: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Inspector")
                .font(.headline)
            if let selected = history.selectedItem {
                Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 6) {
                    FieldRow(title: "Snapshot", value: String(selected.snapshotId))
                    FieldRow(title: "Event", value: selected.eventId.map(String.init))
                    FieldRow(title: "Kind", value: selected.kind)
                    FieldRow(title: "Bytes", value: selected.totalBytes.map(String.init))
                    FieldRow(title: "Matched", value: selected.matchedFields?.joined(separator: ", "))
                    FieldRow(title: "Why", value: selected.whyMatched)
                }
                ItemActionButtons(
                    item: selected,
                    detail: history.selectedDetail,
                    appModel: appModel,
                    onForgot: { await history.forgetSelected() }
                )
            } else {
                Text("Select a result for metadata and actions.")
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding()
    }

    private var searchPrompt: String {
        switch history.mode {
        case .recall: "Describe what you want to recall"
        case .search: "Lexical search"
        case .recent: "Recent mode uses filters"
        case .timeline: "Timeline mode uses filters"
        case .diagnostics: "Diagnostics"
        }
    }

    private func restoreSceneState() {
        history.mode = QueryMode(rawValue: storedMode) ?? .recent
        history.query = storedQuery
        history.selectedID = storedSelectedID == 0 ? nil : storedSelectedID
    }
}
