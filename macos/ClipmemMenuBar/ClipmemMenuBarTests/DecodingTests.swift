import Foundation
import Testing
@testable import ClipmemMenuBar

struct DecodingTests {
    @Test func serviceStatusFixtureDecodesHealth() throws {
        let report = try decode(ServiceStatusReport.self, "service_status")

        #expect(report.health == .healthy)
        #expect(report.launchagent?.running == true)
        #expect(report.retention == "30d")
        #expect(report.dbSizeBytes == 12_582_912)
    }

    @Test func listEnvelopeFixtureDecodesRows() throws {
        let envelope = try decode(ListEnvelope.self, "recent")

        #expect(envelope.command == "recent")
        let firstResult = try #require(envelope.results.first)
        #expect(firstResult.displayText == "git status")
        #expect(firstResult.appHint == "Copied while in Terminal")
    }

    @Test func getFixtureDecodesNestedRepresentations() throws {
        let envelope = try decode(GetEnvelope.self, "get")

        #expect(envelope.snapshot.snapshotId == 7)
        let firstItem = try #require(envelope.snapshot.items.first)
        let firstRepresentation = try #require(firstItem.representations.first)
        let firstEvent = try #require(envelope.snapshot.recentEvents.first)
        #expect(firstRepresentation.uti == "public.utf8-plain-text")
        #expect(firstEvent.frontmostAppName == "Terminal")
    }

    @Test func settingsFixtureDecodesPolicy() throws {
        let settings = try decode(SettingsReport.self, "settings")

        #expect(settings.apiKeyFilterEnabled == true)
        #expect(settings.ocrEnabled == false)
        #expect(settings.ignoredBundleIds.contains("io.openclaw.clipmem.menubar"))
    }

    @Test func sqliteTimestampDisplaysInLocalTimeZone() throws {
        let berlin = try #require(TimeZone(identifier: "Europe/Berlin"))
        let formatted = try #require(DisplayFormatters.localTimestamp(
            "2026-04-19 06:20:00",
            timeZone: berlin,
            locale: Locale(identifier: "en_US_POSIX")
        ))

        #expect(formatted.contains("8:20"))
    }

    @Test func rfc3339TimestampDisplaysInLocalTimeZone() throws {
        let berlin = try #require(TimeZone(identifier: "Europe/Berlin"))
        let formatted = try #require(DisplayFormatters.localTimestamp(
            "2026-04-19T06:20:00Z",
            timeZone: berlin,
            locale: Locale(identifier: "en_US_POSIX")
        ))

        #expect(formatted.contains("8:20"))
    }

    @Test func actionPayloadsDecode() throws {
        let root = try decode([String: JSONValue].self, "actions")
        let data = try JSONSerialization.data(withJSONObject: try object(root["export"]))
        let export = try ClipmemClient.decoder.decode(ExportOutput.self, from: data)

        #expect(export.snapshotId == 7)
        #expect(export.uti == "public.png")
        #expect(export.byteCount == 42)

        let compactData = try JSONSerialization.data(withJSONObject: try object(root["storageCompact"]))
        let compact = try ClipmemClient.decoder.decode(StorageCompactOutput.self, from: compactData)
        #expect(compact.reclaimedBytes == 4096)
        #expect(compact.estimatedReclaimableBytes == 0)
        #expect(compact.checkpoint.busy == 0)

        let optimizeData = try JSONSerialization.data(withJSONObject: try object(root["imageOptimization"]))
        let optimize = try ClipmemClient.decoder.decode(ImageOptimizationOutput.self, from: optimizeData)
        #expect(optimize.format == "webp_lossless")
        #expect(optimize.compactRun == true)
        #expect(optimize.compact?.reclaimedBytes == 393_216)
        #expect(optimize.filesystemSavedBytes == 393_216)
        #expect(optimize.compactRecommended == false)
    }

    private func decode<T: Decodable>(_ type: T.Type, _ name: String) throws -> T {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Fixtures")
            .appendingPathComponent("\(name).json")
        let data = try Data(contentsOf: url)
        return try ClipmemClient.decoder.decode(T.self, from: data)
    }

    private func object(_ value: JSONValue?) throws -> [String: Any] {
        guard case .object(let dictionary) = value else {
            throw FixtureError.expectedObject
        }
        return dictionary.mapValues(any)
    }

    private func any(_ value: JSONValue) -> Any {
        switch value {
        case .string(let value): value
        case .int(let value): value
        case .double(let value): value
        case .bool(let value): value
        case .object(let value): value.mapValues(any)
        case .array(let value): value.map(any)
        case .null: NSNull()
        }
    }

    enum FixtureError: Error {
        case expectedObject
    }
}
