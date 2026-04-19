import Foundation

struct ProviderStatus: Decodable, Equatable, Sendable {
    var provider: String?
    var label: String?
    var state: String?
    var installed: Bool?
    var loaded: Bool?
    var running: Bool?
    var pid: Int?
    var plistPath: String?
    var stdoutLogPath: String?
    var stderrLogPath: String?
}

struct ServiceStatusReport: Decodable, Equatable, Sendable {
    var binaryPath: String?
    var dbPath: String?
    var preferredProvider: String?
    var preferredProviderReason: String?
    var conflict: Bool?
    var homebrew: ProviderStatus?
    var launchagent: ProviderStatus?
    var dbExists: Bool?
    var recentCaptureAt: String?
    var recentCaptureWithinLastHour: Bool?
    var paused: Bool?
    var apiKeyFilterEnabled: Bool?
    var retentionSeconds: UInt64?
    var retention: String?
    var ignoredBundleIdCount: Int?
    var stale: Bool?
    var dbError: String?
    var notes: [String]?

    var health: HealthState {
        if conflict == true { return .conflict }
        if dbError != nil { return .error }
        if dbExists == false { return .setupNeeded }
        if stale == true { return .stale }
        if homebrew?.running == true || launchagent?.running == true { return .healthy }
        if recentCaptureWithinLastHour == true { return .healthy }
        return .setupNeeded
    }

    var logPaths: [String] {
        [homebrew?.stdoutLogPath, homebrew?.stderrLogPath, launchagent?.stdoutLogPath, launchagent?.stderrLogPath]
            .compactMap { $0 }
    }
}

struct DoctorReport: Decodable, Equatable, Sendable {
    var dbPath: String?
    var sqliteVersion: String?
    var journalMode: String?
    var fts5CompileOptionPresent: Bool?
    var fts5CreateVirtualTableOk: Bool?
    var compileOptions: [String]?
}

struct SettingsReport: Decodable, Equatable, Sendable {
    var paused: Bool
    var apiKeyFilterEnabled: Bool
    var ocrEnabled: Bool
    var retentionSeconds: UInt64?
    var retention: String
    var ignoredBundleIds: [String]
}

struct ListEnvelope: Decodable, Equatable, Sendable {
    var schemaVersion: Int?
    var command: String
    var generatedAt: String?
    var appliedFilters: [String: JSONValue]?
    var truncated: Bool
    var nextCursor: String?
    var results: [ClipmemItem]
}

struct RecallEnvelope: Decodable, Equatable, Sendable {
    var schemaVersion: Int?
    var command: String
    var generatedAt: String?
    var query: String?
    var bestCandidate: ClipmemItem
    var alternatives: [ClipmemItem]
    var bestMatchConfidence: String?
    var bestMatchScore: Double?
    var whySelected: String?
    var quotedText: String?
}

struct GetEnvelope: Decodable, Equatable, Sendable {
    var schemaVersion: Int?
    var command: String
    var generatedAt: String?
    var snapshot: SnapshotDetails
}

struct ClipmemItem: Decodable, Identifiable, Hashable, Sendable {
    var snapshotId: Int
    var eventId: Int?
    var sha256: String?
    var kind: String?
    var observedAt: String?
    var firstSeenAt: String?
    var lastSeenAt: String?
    var appName: String?
    var appBundleId: String?
    var bestText: String?
    var bestTextUti: String?
    var textFragments: [TextFragment]?
    var urls: [String]?
    var filePaths: [String]?
    var htmlText: String?
    var rtfText: String?
    var textSummary: String?
    var previewText: String?
    var itemCount: Int?
    var totalBytes: Int?
    var captureCount: Int?
    var score: Double?
    var whyMatched: String?
    var matchedFields: [String]?
    var snippet: String?
    var changeCount: Int?

    var id: String { "\(eventId ?? snapshotId)-\(snapshotId)" }

    var displayText: String {
        let candidate = [snippet, bestText, previewText, textSummary].compactMap { $0 }.first { $0.isEmpty == false }
        return candidate ?? "[No extracted text]"
    }

    var appHint: String? {
        guard let appName, appName.isEmpty == false else { return nil }
        return "Copied while in \(appName)"
    }

    var hasText: Bool {
        bestText?.isEmpty == false || previewText?.isEmpty == false || textSummary?.isEmpty == false
    }

    var copyablePlainText: String? {
        [bestText, previewText]
            .compactMap { $0 }
            .first { $0.isEmpty == false }
    }
}

struct TextFragment: Decodable, Hashable, Sendable {
    var itemIndex: Int?
    var uti: String?
    var kind: String?
    var text: String?
}

struct SnapshotDetails: Decodable, Equatable, Sendable {
    var snapshotId: Int
    var sha256: String
    var snapshotKind: String?
    var bestText: String?
    var bestTextUti: String?
    var textFragments: [TextFragment]?
    var urls: [String]
    var filePaths: [String]
    var htmlText: String?
    var rtfText: String?
    var textSummary: String?
    var previewText: String?
    var searchText: String?
    var itemCount: Int
    var totalBytes: Int
    var createdAt: String?
    var captureCount: Int
    var firstObservedAt: String?
    var lastObservedAt: String?
    var lastFrontmostAppName: String?
    var lastFrontmostAppBundleId: String?
    var recentEvents: [CaptureEvent]
    var items: [ClipboardItemDetail]
}

struct CaptureEvent: Decodable, Equatable, Identifiable, Sendable {
    var eventId: Int
    var observedAt: String
    var changeCount: Int?
    var frontmostAppName: String?
    var frontmostAppBundleId: String?

    var id: Int { eventId }
}

struct ClipboardItemDetail: Decodable, Equatable, Identifiable, Sendable {
    var itemIndex: Int
    var primaryKind: String?
    var primaryUti: String?
    var previewText: String?
    var searchText: String?
    var totalBytes: Int
    var representations: [ClipboardRepresentation]

    var id: Int { itemIndex }
}

struct ClipboardRepresentation: Decodable, Equatable, Identifiable, Sendable {
    var uti: String
    var kind: String?
    var isText: Bool?
    var byteLen: Int
    var rawSha256: String?
    var textValue: String?

    var id: String { uti }
}

struct RestoreOutput: Decodable, Equatable, Sendable {
    var snapshotId: Int
    var itemCount: Int
    var representationCount: Int
    var totalBytes: Int
}

struct ExportOutput: Decodable, Equatable, Sendable {
    var snapshotId: Int
    var itemIndex: Int
    var uti: String
    var byteCount: Int
    var rawSha256: String
    var out: String
}

struct ForgetOutput: Decodable, Equatable, Sendable {
    var snapshotId: Int
    var itemCount: Int
    var representationCount: Int
    var captureEventCount: Int
    var totalBytes: Int
}

struct PurgeOutput: Decodable, Equatable, Sendable {
    var olderThanSeconds: UInt64
    var dryRun: Bool
    var snapshotCount: Int
    var itemCount: Int
    var representationCount: Int
    var captureEventCount: Int
    var totalBytes: Int
}

enum JSONValue: Decodable, Equatable, Hashable, Sendable {
    case string(String)
    case int(Int)
    case double(Double)
    case bool(Bool)
    case object([String: JSONValue])
    case array([JSONValue])
    case null

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Int.self) {
            self = .int(value)
        } else if let value = try? container.decode(Double.self) {
            self = .double(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([String: JSONValue].self) {
            self = .object(value)
        } else {
            self = .array(try container.decode([JSONValue].self))
        }
    }
}
