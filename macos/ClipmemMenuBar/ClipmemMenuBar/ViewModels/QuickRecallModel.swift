import Foundation
import Observation

@MainActor
@Observable
final class QuickRecallModel {
    var mode: QueryMode = .recall
    var query = ""
    var results: [ClipmemItem] = []
    var selectedID: Int?
    var isLoading = false
    var error: UserError?

    var errorMessage: String? { error?.message }

    @ObservationIgnored private let appModel: AppModel
    @ObservationIgnored private var searchTask: Task<Void, Never>?
    @ObservationIgnored private let forgetItem: @MainActor (ClipmemItem) async -> Bool

    init(appModel: AppModel, forgetItem: (@MainActor (ClipmemItem) async -> Bool)? = nil) {
        self.appModel = appModel
        self.forgetItem = forgetItem ?? { item in
            await appModel.forget(item)
        }
    }

    var selectedItem: ClipmemItem? {
        guard let selectedID else { return nil }
        return results.first { $0.snapshotId == selectedID }
    }

    func queryChanged() {
        searchTask?.cancel()
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(180))
            guard Task.isCancelled == false else { return }
            await self?.refresh()
        }
    }

    func refresh() async {
        isLoading = true
        defer { isLoading = false }
        do {
            let filters = RetrievalFilterState.defaultValue
            let newResults: [ClipmemItem]
            switch mode {
            case .recall:
                let envelope = try await appModel.client.recall(query: query.isEmpty ? nil : query, limit: 12, filters: filters)
                newResults = [envelope.bestCandidate] + envelope.alternatives
            case .search:
                guard query.isEmpty == false else {
                    results = []
                    selectedID = nil
                    return
                }
                newResults = try await appModel.client.search(query: query, limit: 20, cursor: nil, filters: filters).results
            case .recent:
                newResults = try await appModel.client.recent(limit: 20, cursor: nil, filters: filters).results
            case .timeline:
                newResults = try await appModel.client.timeline(limit: 20, cursor: nil, filters: filters).results
            case .diagnostics:
                newResults = []
            }
            if Task.isCancelled { return }
            results = newResults
            selectedID = results.first?.snapshotId
            self.error = nil
        } catch is CancellationError {
        } catch {
            self.error = UserError(error)
        }
    }

    func moveSelection(_ delta: Int) {
        guard results.isEmpty == false else { return }
        let current = selectedID.flatMap { id in results.firstIndex { $0.snapshotId == id } } ?? 0
        let next = min(max(current + delta, 0), results.count - 1)
        selectedID = results[next].snapshotId
    }

    func restoreSelected() async {
        guard let selectedItem else { return }
        await appModel.restore(selectedItem)
    }

    func forgetSelected() async {
        guard let selectedItem else { return }
        await forget(selectedItem)
    }

    func forget(_ item: ClipmemItem) async {
        guard await forgetItem(item) else { return }
        results.removeAll { $0.snapshotId == item.snapshotId }
        if selectedID == item.snapshotId {
            selectedID = results.first?.snapshotId
        }
    }
}
