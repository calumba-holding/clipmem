import SwiftUI

struct HistoryWindowView: View {
    let appModel: AppModel

    @Environment(\.openWindow) private var openWindow
    @State private var history: HistoryModel
    @SceneStorage("history.mode") private var storedMode = QueryMode.recent.rawValue
    @SceneStorage("history.query") private var storedQuery = ""
    @SceneStorage("history.inspector") private var inspectorPresented = false
    @SceneStorage("history.selected") private var storedSelectedID = 0
    @State private var handledHistoryOpenRequestID = 0
    @State private var displayMode: DisplayMode = .recent
    @State private var searchStyle: SearchStyle = .smart

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
        .navigationTitle(sidebarSelection == .diagnostics ? "Diagnostics" : displayMode.title)
        .toolbar {
            ToolbarItemGroup {
                Button("Refresh", systemImage: "arrow.clockwise") {
                    Task { await history.reload() }
                }
                .keyboardShortcut("r", modifiers: .command)
                Button("Search", systemImage: "magnifyingglass") {
                    WindowActivation.openWindow(openWindow, id: .quickRecall)
                }
                Button("Inspector", systemImage: inspectorPresented ? "sidebar.right.fill" : "sidebar.right") {
                    inspectorPresented.toggle()
                }
                .help("Toggle inspector (\u{2318}\u{21E7}I)")
            }
        }
        .inspector(isPresented: $inspectorPresented) {
            inspector
                .inspectorColumnWidth(min: 220, ideal: 260, max: 320)
        }
        .overlay(alignment: .top) {
            ActionFeedbackOverlay(message: appModel.actionMessage)
                .padding(.top, Spacing.sm)
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
            if await applyPendingHistoryOpenRequestIfNeeded() == false {
                await history.reload()
            }
        }
        .onChange(of: appModel.pendingHistoryOpenRequest?.id) {
            Task {
                await applyPendingHistoryOpenRequestIfNeeded()
            }
        }
        .onChange(of: history.query) {
            storedQuery = history.query
        }
        .onChange(of: history.selectedID) {
            storedSelectedID = history.selectedID ?? 0
            Task { await history.loadSelectedDetail() }
        }
        .onChange(of: appModel.clipboardHistoryRevision) {
            Task { await history.refreshForExternalHistoryChange() }
        }
    }

    // MARK: - Sidebar

    /// Sidebar selection uses an enum that covers both DisplayMode items and diagnostics.
    private enum SidebarItem: String, Hashable {
        case search, recent, timeline, diagnostics

        var displayMode: DisplayMode? {
            switch self {
            case .search: .search
            case .recent: .recent
            case .timeline: .timeline
            case .diagnostics: nil
            }
        }

        static func from(displayMode: DisplayMode) -> SidebarItem {
            switch displayMode {
            case .search: .search
            case .recent: .recent
            case .timeline: .timeline
            }
        }
    }

    @State private var sidebarSelection: SidebarItem = .recent

    private var sidebar: some View {
        List(selection: sidebarBinding) {
            Section("Browse") {
                ForEach(DisplayMode.allCases) { mode in
                    Label(mode.title, systemImage: mode.symbol)
                        .tag(SidebarItem.from(displayMode: mode))
                }
            }
            Section("System") {
                Label(QueryMode.diagnostics.title, systemImage: QueryMode.diagnostics.symbol)
                    .tag(SidebarItem.diagnostics)
            }
        }
        .navigationTitle("clipmem")
        .navigationSplitViewColumnWidth(min: 150, ideal: 180, max: 220)
    }

    private var sidebarBinding: Binding<SidebarItem> {
        Binding {
            sidebarSelection
        } set: { newItem in
            guard sidebarSelection != newItem else { return }
            sidebarSelection = newItem
            if let dm = newItem.displayMode {
                displayMode = dm
                syncMode()
            } else {
                // Diagnostics
                history.mode = .diagnostics
            }
            storedMode = history.mode.rawValue
            Task { await history.reload() }
        }
    }

    // MARK: - Content Column

    @ViewBuilder
    private var contentColumn: some View {
        if sidebarSelection == .diagnostics {
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
            .navigationTitle(displayMode.title)
            .navigationSplitViewColumnWidth(min: 320, ideal: 420, max: 560)
        }
    }

    @ViewBuilder
    private var detailColumn: some View {
        if sidebarSelection == .diagnostics {
            EmptyStateView(title: "Diagnostics", detail: "Service and doctor output are shown in the middle column.", symbol: "stethoscope")
                .navigationTitle("Details")
                .navigationSplitViewColumnWidth(min: 360, ideal: 520)
        } else {
            SnapshotDetailView(detail: history.selectedDetail, fallback: history.selectedItem, isLoading: history.isLoadingDetail)
                .navigationTitle(displayMode.title)
                .navigationSplitViewColumnWidth(min: 360, ideal: 580)
        }
    }

    // MARK: - Query Controls

    private var queryControls: some View {
        VStack(spacing: Spacing.md) {
            HStack(spacing: Spacing.sm) {
                if displayMode == .search {
                    Picker("Style", selection: $searchStyle) {
                        Text("Smart").tag(SearchStyle.smart)
                        Text("Exact").tag(SearchStyle.exact)
                    }
                    .pickerStyle(.segmented)
                    .fixedSize()
                    .controlSize(.small)
                    .onChange(of: searchStyle) {
                        syncMode()
                        Task { await history.reload() }
                    }
                }

                TextField(searchPrompt, text: $history.query)
                    .textFieldStyle(.roundedBorder)
                    .disabled(displayMode == .recent || displayMode == .timeline)
                    .onSubmit {
                        Task { await history.reload() }
                    }
                Button("Search", systemImage: "magnifyingglass") {
                    Task { await history.reload() }
                }
                .disabled(displayMode == .search && history.query.isEmpty)
            }
            FilterBar(history: history)
        }
    }

    private var searchPrompt: String {
        switch displayMode {
        case .search:
            searchStyle == .smart ? "Describe what you want to recall" : "Search for exact text"
        case .recent:
            "Recent mode uses filters"
        case .timeline:
            "Timeline mode uses filters"
        }
    }

    // MARK: - Results

    private var resultList: some View {
        VStack(spacing: 0) {
            if let error = history.error {
                ErrorBanner(message: error.message, recovery: error.recovery)
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
                    .padding(.vertical, Spacing.sm)
                }
            }
            .listStyle(.inset)
            .overlay {
                if !history.isLoading && history.results.isEmpty && history.error == nil {
                    EmptyStateView(
                        title: displayMode == .recent || displayMode == .timeline ? "No recent history" : "No results",
                        detail: displayMode == .recent || displayMode == .timeline
                            ? "Start copying to build your clipboard history."
                            : "Try adjusting your filters or search query.",
                        symbol: displayMode == .recent || displayMode == .timeline ? "clock" : "magnifyingglass"
                    )
                }
            }
            if history.isLoading {
                ProgressView()
                    .padding(Spacing.sm)
            }
        }
    }

    // MARK: - Inspector

    private var inspector: some View {
        VStack(alignment: .leading, spacing: Spacing.lg) {
            Text("Inspector")
                .font(.headline)
            if let selected = history.selectedItem {
                Grid(alignment: .leading, horizontalSpacing: Spacing.md, verticalSpacing: Spacing.sm) {
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

    // MARK: - State Management

    private func syncMode() {
        history.mode = displayMode.queryMode(searchStyle: searchStyle)
        storedMode = history.mode.rawValue
    }

    private func restoreSceneState() {
        let queryMode = QueryMode(rawValue: storedMode) ?? .recent
        let (dm, ss) = DisplayMode.from(queryMode: queryMode)
        displayMode = dm
        searchStyle = ss
        sidebarSelection = queryMode == .diagnostics ? .diagnostics : SidebarItem.from(displayMode: dm)
        history.mode = queryMode
        history.query = storedQuery
        history.selectedID = storedSelectedID == 0 ? nil : storedSelectedID
    }

    @discardableResult
    private func applyPendingHistoryOpenRequestIfNeeded() async -> Bool {
        guard let request = appModel.pendingHistoryOpenRequest else { return false }
        guard request.id != handledHistoryOpenRequestID else { return false }

        handledHistoryOpenRequestID = request.id

        if request.mode == .diagnostics {
            sidebarSelection = .diagnostics
            history.mode = .diagnostics
        } else {
            let (dm, ss) = DisplayMode.from(queryMode: request.mode)
            displayMode = dm
            searchStyle = ss
            sidebarSelection = SidebarItem.from(displayMode: dm)
            history.mode = request.mode
        }

        history.query = request.query
        storedMode = history.mode.rawValue
        storedQuery = request.query
        storedSelectedID = request.focusedSnapshotID ?? 0
        await history.reload(selecting: request.focusedSnapshotID)
        return true
    }
}

private struct HistoryWindowWidthKey: SwiftUI.PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}
