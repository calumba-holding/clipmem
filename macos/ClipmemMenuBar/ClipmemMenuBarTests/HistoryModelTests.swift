import Foundation
import Testing
@testable import ClipmemMenuBar

struct HistoryModelTests {
    @Test
    @MainActor
    func requestHistorySearchTrimsQueryAndCreatesSearchRequest() throws {
        let appModel = AppModel(loadRecentPreview: { [] })

        appModel.requestHistorySearch(query: "  release notes  ")

        let request = try #require(appModel.pendingHistoryOpenRequest)
        #expect(request.id == 1)
        #expect(request.mode == .search)
        #expect(request.query == "release notes")
        #expect(request.focusedSnapshotID == nil)
    }

    @Test
    @MainActor
    func requestHistoryFocusRecordsSnapshotAndSourceContext() throws {
        let appModel = AppModel(loadRecentPreview: { [] })

        appModel.requestHistoryFocus(snapshotID: 42, mode: .recall, query: "  ssh command  ")

        let request = try #require(appModel.pendingHistoryOpenRequest)
        #expect(request.id == 1)
        #expect(request.mode == .recall)
        #expect(request.query == "ssh command")
        #expect(request.focusedSnapshotID == 42)
    }

    @Test
    @MainActor
    func requestHistoryFocusCoercesDiagnosticsToRecent() throws {
        let appModel = AppModel(loadRecentPreview: { [] })

        appModel.requestHistoryFocus(snapshotID: 42, mode: .diagnostics, query: "ignored")

        let request = try #require(appModel.pendingHistoryOpenRequest)
        #expect(request.mode == .recent)
        #expect(request.query == "")
        #expect(request.focusedSnapshotID == 42)
    }

    @Test
    func diagnosticsModeIsNotHistoryCompatible() {
        #expect(QueryMode.diagnostics.historyCompatibleMode == .recent)
        #expect(QueryMode.search.historyCompatibleMode == .search)
    }

    @Test
    @MainActor
    func requestSettingsTabRecordsTabAndAdvancesID() throws {
        let appModel = AppModel(loadRecentPreview: { [] })

        appModel.requestSettingsTab(.diagnostics)
        let firstRequest = try #require(appModel.pendingSettingsOpenRequest)

        appModel.requestSettingsTab(.storage)
        let secondRequest = try #require(appModel.pendingSettingsOpenRequest)

        #expect(firstRequest.tab == .diagnostics)
        #expect(secondRequest.id == firstRequest.id + 1)
        #expect(secondRequest.tab == .storage)
    }

    @Test
    @MainActor
    func repeatedHistorySearchRequestsKeepSearchModeAndAdvanceID() throws {
        let appModel = AppModel(loadRecentPreview: { [] })

        appModel.requestHistorySearch(query: "first")
        let firstRequest = try #require(appModel.pendingHistoryOpenRequest)

        appModel.requestHistorySearch(query: "  second  ")

        let secondRequest = try #require(appModel.pendingHistoryOpenRequest)
        #expect(secondRequest.id == firstRequest.id + 1)
        #expect(secondRequest.mode == .search)
        #expect(secondRequest.query == "second")
        #expect(secondRequest.focusedSnapshotID == nil)
    }

    @Test
    @MainActor
    func reloadSelectingKeepsFocusedSnapshotWhenItAppearsInResults() async throws {
        let appModel = AppModel(loadRecentPreview: { [] })
        var detailRequests: [Int] = []
        let history = HistoryModel(
            mode: .search,
            appModel: appModel,
            pageLoader: { mode, query, _, cursor in
                #expect(mode == .search)
                #expect(query == "snippet")
                #expect(cursor == nil)
                return ([Self.item(snapshotID: 1), Self.item(snapshotID: 7)], nil)
            },
            detailLoader: { snapshotID in
                detailRequests.append(snapshotID)
                return Self.detail(snapshotID: snapshotID)
            }
        )
        history.query = "snippet"

        await history.reload(selecting: 7)

        #expect(history.selectedID == 7)
        #expect(history.selectedItem?.snapshotId == 7)
        #expect(history.selectedDetail?.snapshotId == 7)
        #expect(detailRequests == [7])
    }

    @Test
    @MainActor
    func reloadSelectingLoadsFocusedDetailWhenSnapshotIsNotInResults() async throws {
        let appModel = AppModel(loadRecentPreview: { [] })
        var detailRequests: [Int] = []
        let history = HistoryModel(
            mode: .recent,
            appModel: appModel,
            pageLoader: { _, _, _, _ in
                ([Self.item(snapshotID: 1), Self.item(snapshotID: 2)], nil)
            },
            detailLoader: { snapshotID in
                detailRequests.append(snapshotID)
                return Self.detail(snapshotID: snapshotID)
            }
        )

        await history.reload(selecting: 7)

        #expect(history.selectedID == 7)
        #expect(history.selectedItem == nil)
        #expect(history.selectedDetail?.snapshotId == 7)
        #expect(detailRequests == [7])
    }

    private static func item(snapshotID: Int) -> ClipmemItem {
        ClipmemItem(
            snapshotId: snapshotID,
            eventId: nil,
            sha256: nil,
            kind: "text",
            observedAt: nil,
            firstSeenAt: nil,
            lastSeenAt: nil,
            appName: nil,
            appBundleId: nil,
            bestText: "Snapshot \(snapshotID)",
            bestTextUti: nil,
            textFragments: nil,
            urls: nil,
            filePaths: nil,
            htmlText: nil,
            rtfText: nil,
            textSummary: nil,
            previewText: nil,
            itemCount: nil,
            totalBytes: nil,
            captureCount: nil,
            score: nil,
            whyMatched: nil,
            matchedFields: nil,
            snippet: nil,
            changeCount: nil
        )
    }

    private static func detail(snapshotID: Int) -> SnapshotDetails {
        SnapshotDetails(
            snapshotId: snapshotID,
            sha256: "sha-\(snapshotID)",
            snapshotKind: "text",
            bestText: "Snapshot \(snapshotID)",
            bestTextUti: nil,
            textFragments: nil,
            urls: [],
            filePaths: [],
            htmlText: nil,
            rtfText: nil,
            textSummary: nil,
            previewText: nil,
            searchText: nil,
            itemCount: 1,
            totalBytes: 16,
            createdAt: nil,
            captureCount: 1,
            firstObservedAt: nil,
            lastObservedAt: nil,
            lastFrontmostAppName: nil,
            lastFrontmostAppBundleId: nil,
            recentEvents: [],
            items: []
        )
    }
}
