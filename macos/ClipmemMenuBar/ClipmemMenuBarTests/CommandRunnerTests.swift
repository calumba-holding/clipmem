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
struct QuickRecallModelTests {
    @Test func forgetExplicitItemDoesNotDependOnSelection() async {
        var forgottenIDs: [Int] = []
        let model = QuickRecallModel(appModel: AppModel()) { item in
            forgottenIDs.append(item.snapshotId)
        }
        model.results = [Self.item(1), Self.item(2)]
        model.selectedID = 1

        await model.forget(Self.item(2))

        #expect(forgottenIDs == [2])
        #expect(model.results.map(\.snapshotId) == [1])
        #expect(model.selectedID == 1)
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
