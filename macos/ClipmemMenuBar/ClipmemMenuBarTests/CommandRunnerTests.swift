import Foundation
import Testing
@testable import ClipmemMenuBar

struct CommandRunnerTests {
    @Test func drainsLargeStdoutAndStderrBeforeWaiting() async throws {
        let byteCount = 200_000
        let script = "print \"o\" x \(byteCount); print STDERR \"e\" x \(byteCount);"

        let result = try await CommandRunner().run(executable: "/usr/bin/perl", arguments: ["-e", script])

        #expect(result.exitCode == 0)
        #expect(result.stdout.count == byteCount)
        #expect(result.stderr.count == byteCount)
    }

    @Test func cancellationTerminatesRunningProcess() async throws {
        let task = Task {
            try await CommandRunner().run(executable: "/bin/sh", arguments: ["-c", "sleep 30"])
        }

        try await Task.sleep(for: .milliseconds(100))
        task.cancel()

        do {
            _ = try await task.value
            Issue.record("Expected cancellation to throw.")
        } catch is CancellationError {
        } catch {
            Issue.record("Expected CancellationError, got \(error).")
        }
    }
}

@MainActor
struct ReactiveRefreshTests {
    @Test func pasteboardMonitorEmitsOnlyWhenChangeCountChanges() {
        let changeCount = IntBox(1)
        let emittedChanges = IntBox(0)
        let monitor = PasteboardChangeMonitor(
            changeCount: { changeCount.value },
            onChange: { emittedChanges.value += 1 }
        )

        monitor.pollOnce()
        monitor.pollOnce()
        changeCount.value = 2
        monitor.pollOnce()
        monitor.pollOnce()

        #expect(emittedChanges.value == 1)
    }

    @Test func pasteboardMonitorCanMarkCurrentChangeHandled() {
        let changeCount = IntBox(1)
        let emittedChanges = IntBox(0)
        let monitor = PasteboardChangeMonitor(
            changeCount: { changeCount.value },
            onChange: { emittedChanges.value += 1 }
        )

        monitor.pollOnce()
        changeCount.value = 2
        monitor.markCurrentChangeHandled()
        monitor.pollOnce()

        #expect(emittedChanges.value == 0)
    }

    @Test func recentRefreshCoordinatorCoalescesRapidChanges() async {
        var refreshCount = 0
        let coordinator = RecentPreviewRefreshCoordinator(
            sleep: { _ in },
            refresh: {
                refreshCount += 1
                return true
            }
        )

        coordinator.schedule()
        coordinator.schedule()
        coordinator.schedule()
        await Self.drainScheduledTasks()

        #expect(refreshCount == 1)
    }

    @Test func recentRefreshCoordinatorQueuesOneFollowUpWhileRefreshing() async {
        var refreshCount = 0
        var firstRefreshContinuation: CheckedContinuation<Void, Never>?
        let coordinator = RecentPreviewRefreshCoordinator(
            sleep: { _ in },
            refresh: {
                refreshCount += 1
                if refreshCount == 1 {
                    await withCheckedContinuation { continuation in
                        firstRefreshContinuation = continuation
                    }
                }
                return true
            }
        )

        coordinator.schedule()
        await Self.drainScheduledTasks()

        coordinator.schedule()
        coordinator.schedule()
        await Self.drainScheduledTasks()
        #expect(refreshCount == 1)

        firstRefreshContinuation?.resume()
        await Self.drainScheduledTasks()

        #expect(refreshCount == 2)
    }

    @Test func staleRecentPreviewRefreshIncrementsRevisionOnlyWhenItRefreshes() async {
        var loadCount = 0
        let appModel = AppModel {
            loadCount += 1
            return [Self.item(9)]
        }

        await appModel.refreshRecentPreviewIfStale(maxAge: 1)
        await appModel.refreshRecentPreviewIfStale(maxAge: 60)

        #expect(loadCount == 1)
        #expect(appModel.clipboardHistoryRevision == 1)
        #expect(appModel.recentPreview.map(\.snapshotId) == [9])
    }

    @Test func recentPreviewRefreshReportsOnlyActualListChanges() async {
        var loads = [[Self.item(9)], [Self.item(9)], [Self.item(10)]]
        let appModel = AppModel {
            loads.removeFirst()
        }

        let firstChanged = await appModel.refreshRecentPreview()
        let secondChanged = await appModel.refreshRecentPreview()
        let thirdChanged = await appModel.refreshRecentPreview()

        #expect(firstChanged)
        #expect(!secondChanged)
        #expect(thirdChanged)
        #expect(appModel.recentPreview.map(\.snapshotId) == [10])
    }

    private static func drainScheduledTasks() async {
        for _ in 0..<5 {
            await Task.yield()
        }
    }

    private static func item(_ snapshotID: Int) -> ClipmemItem {
        ClipmemItem(
            snapshotId: snapshotID,
            eventId: nil,
            sha256: nil,
            kind: nil,
            observedAt: nil,
            firstSeenAt: nil,
            lastSeenAt: nil,
            appName: nil,
            appBundleId: nil,
            bestText: nil,
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

    private final class IntBox {
        var value: Int

        init(_ value: Int) {
            self.value = value
        }
    }
}

@MainActor
struct HistoryExternalRefreshTests {
    @Test(arguments: [QueryMode.recent, .timeline])
    func externalHistoryRefreshReloadsBrowseModesAndPreservesSelection(mode: QueryMode) async {
        var requestedModes: [QueryMode] = []
        let history = HistoryModel(mode: mode, appModel: AppModel()) { mode, _, _, _ in
            requestedModes.append(mode)
            return ([Self.item(3), Self.item(2), Self.item(1)], "next")
        }
        history.results = [Self.item(2), Self.item(1)]
        history.selectedID = 2

        await history.refreshForExternalHistoryChange()

        #expect(requestedModes == [mode])
        #expect(history.results.map(\.snapshotId) == [3, 2, 1])
        #expect(history.nextCursor == "next")
        #expect(history.selectedID == 2)
    }

    @Test func externalHistoryRefreshSelectsNewestWhenPreviousSelectionDisappears() async {
        let history = HistoryModel(mode: .recent, appModel: AppModel()) { _, _, _, _ in
            ([Self.item(4), Self.item(3)], nil)
        }
        history.results = [Self.item(2), Self.item(1)]
        history.selectedID = 2

        await history.refreshForExternalHistoryChange()

        #expect(history.results.map(\.snapshotId) == [4, 3])
        #expect(history.selectedID == 4)
        #expect(history.selectedDetail == nil)
    }

    @Test(arguments: [QueryMode.recall, .search, .diagnostics])
    func externalHistoryRefreshIgnoresUserDrivenModes(mode: QueryMode) async {
        var loadCount = 0
        let history = HistoryModel(mode: mode, appModel: AppModel()) { _, _, _, _ in
            loadCount += 1
            return ([Self.item(3)], nil)
        }
        history.results = [Self.item(1)]
        history.selectedID = 1

        await history.refreshForExternalHistoryChange()

        #expect(loadCount == 0)
        #expect(history.results.map(\.snapshotId) == [1])
        #expect(history.selectedID == 1)
    }

    private static func item(_ snapshotID: Int) -> ClipmemItem {
        ClipmemItem(
            snapshotId: snapshotID,
            eventId: nil,
            sha256: nil,
            kind: nil,
            observedAt: nil,
            firstSeenAt: nil,
            lastSeenAt: nil,
            appName: nil,
            appBundleId: nil,
            bestText: nil,
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
}

@MainActor
struct QuickRecallModelTests {
    @Test func forgetExplicitItemDoesNotDependOnSelection() async {
        var forgottenIDs: [Int] = []
        let model = QuickRecallModel(appModel: AppModel()) { item in
            forgottenIDs.append(item.snapshotId)
            return true
        }
        model.results = [Self.item(1), Self.item(2)]
        model.selectedID = 1

        await model.forget(Self.item(2))

        #expect(forgottenIDs == [2])
        #expect(model.results.map(\.snapshotId) == [1])
        #expect(model.selectedID == 1)
    }

    @Test func failedForgetLeavesResultsAndSelectionUnchanged() async {
        var forgottenIDs: [Int] = []
        let model = QuickRecallModel(appModel: AppModel()) { item in
            forgottenIDs.append(item.snapshotId)
            return false
        }
        model.results = [Self.item(1), Self.item(2)]
        model.selectedID = 2

        await model.forget(Self.item(2))

        #expect(forgottenIDs == [2])
        #expect(model.results.map(\.snapshotId) == [1, 2])
        #expect(model.selectedID == 2)
    }

    @Test func copyablePlainTextUsesFirstNonEmptyTextValue() {
        #expect(Self.item(1, bestText: "plain", previewText: "preview").copyablePlainText == "plain")
        #expect(Self.item(1, bestText: nil, previewText: "preview").copyablePlainText == "preview")
        #expect(Self.item(1, bestText: "", previewText: "preview").copyablePlainText == "preview")
        #expect(Self.item(1, bestText: nil, previewText: nil).copyablePlainText == nil)
        #expect(Self.item(1, bestText: "", previewText: "").copyablePlainText == nil)
    }

    private static func item(_ snapshotID: Int, bestText: String? = nil, previewText: String? = nil) -> ClipmemItem {
        ClipmemItem(
            snapshotId: snapshotID,
            eventId: nil,
            sha256: nil,
            kind: nil,
            observedAt: nil,
            firstSeenAt: nil,
            lastSeenAt: nil,
            appName: nil,
            appBundleId: nil,
            bestText: bestText,
            bestTextUti: nil,
            textFragments: nil,
            urls: nil,
            filePaths: nil,
            htmlText: nil,
            rtfText: nil,
            textSummary: nil,
            previewText: previewText,
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
}
