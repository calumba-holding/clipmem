import SwiftUI

struct HistoryWindowView: View {
    let appModel: AppModel

    @Environment(\.openWindow) private var openWindow
    @State private var history: HistoryModel
    @SceneStorage("history.mode") private var storedMode = QueryMode.recent.rawValue
    @SceneStorage("history.query") private var storedQuery = ""
    @SceneStorage("history.inspector") private var inspectorPresented = false
    @SceneStorage("history.selected") private var storedSelectedID = 0

    init(appModel: AppModel) {
        self.appModel = appModel
        _history = State(initialValue: HistoryModel(appModel: appModel))
    }

    var body: some View {
        NavigationSplitView {
            sidebar
        } content: {
            contentColumn
        } detail: {
            detailColumn
        }
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
        .inspector(isPresented: $inspectorPresented) {
            inspector
                .inspectorColumnWidth(min: 220, ideal: 260, max: 320)
        }
        .navigationSplitViewStyle(.balanced)
        .background {
            GeometryReader { proxy in
                Color.clear.preference(key: HistoryWindowWidthKey.self, value: proxy.size.width)
            }
        }
        .onPreferenceChange(HistoryWindowWidthKey.self) { width in
            if inspectorPresented, width < 1_400 {
                inspectorPresented = false
            }
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

    private var sidebar: some View {
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
        .navigationSplitViewColumnWidth(min: 150, ideal: 180, max: 220)
    }

    @ViewBuilder
    private var contentColumn: some View {
        if history.mode == .diagnostics {
            DiagnosticsView(appModel: appModel)
                .navigationTitle("Diagnostics")
                .navigationSplitViewColumnWidth(min: 560, ideal: 700)
        } else {
            VStack(spacing: 0) {
                queryControls
                    .padding()
                Divider()
                resultList
            }
            .navigationTitle(history.mode.title)
            .navigationSplitViewColumnWidth(min: 320, ideal: 420, max: 560)
        }
    }

    @ViewBuilder
    private var detailColumn: some View {
        if history.mode == .diagnostics {
            EmptyStateView(title: "Diagnostics", detail: "Service and doctor output are shown in the middle column.", symbol: "stethoscope")
                .navigationTitle("Details")
                .navigationSplitViewColumnWidth(min: 360, ideal: 520)
        } else {
            SnapshotDetailView(detail: history.selectedDetail, fallback: history.selectedItem)
                .navigationTitle(history.mode.title)
                .navigationSplitViewColumnWidth(min: 360, ideal: 580)
        }
    }

    private var queryControls: some View {
        ViewThatFits(in: .horizontal) {
            wideQueryControls
            compactQueryControls
        }
    }

    private var wideQueryControls: some View {
        VStack(spacing: 10) {
            HStack {
                Picker("Mode", selection: $history.mode) {
                    ForEach([QueryMode.recall, .search, .recent, .timeline]) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 300)
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
    }

    private var compactQueryControls: some View {
        VStack(alignment: .leading, spacing: 10) {
            Picker("Mode", selection: $history.mode) {
                ForEach([QueryMode.recall, .search, .recent, .timeline]) { mode in
                    Text(mode.title).tag(mode)
                }
            }
            .pickerStyle(.segmented)
            .frame(width: 300)

            HStack {
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

private struct HistoryWindowWidthKey: SwiftUI.PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}
