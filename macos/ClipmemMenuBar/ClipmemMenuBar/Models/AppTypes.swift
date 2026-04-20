import Foundation
import SwiftUI

enum QueryMode: String, CaseIterable, Identifiable, Codable, Hashable, Sendable {
    case recall
    case search
    case recent
    case timeline
    case diagnostics

    var id: String { rawValue }

    var title: String {
        switch self {
        case .recall: "Recall"
        case .search: "Search"
        case .recent: "Recent"
        case .timeline: "Timeline"
        case .diagnostics: "Diagnostics"
        }
    }

    var symbol: String {
        switch self {
        case .recall: "sparkle.magnifyingglass"
        case .search: "magnifyingglass"
        case .recent: "clock"
        case .timeline: "list.bullet.rectangle"
        case .diagnostics: "stethoscope"
        }
    }
}

enum HealthState: String, Sendable {
    case healthy
    case capturePaused
    case watcherStopped
    case noRecentCaptures
    case setupNeeded
    case conflict
    case missingBinary
    case error
    case unknown

    var title: String {
        switch self {
        case .healthy: "Capture Running"
        case .capturePaused: "Capture Paused"
        case .watcherStopped: "Watcher Stopped"
        case .noRecentCaptures: "No Recent Captures"
        case .setupNeeded: "Setup Needed"
        case .conflict: "Service Conflict"
        case .missingBinary: "Binary Missing"
        case .error: "Needs Attention"
        case .unknown: "Checking"
        }
    }

    var symbol: String {
        switch self {
        case .healthy: "checkmark.circle.fill"
        case .capturePaused: "pause.circle.fill"
        case .watcherStopped: "stop.circle.fill"
        case .noRecentCaptures: "clock.arrow.circlepath"
        case .setupNeeded: "wrench.and.screwdriver"
        case .conflict: "exclamationmark.triangle.fill"
        case .missingBinary: "questionmark.folder"
        case .error: "xmark.octagon.fill"
        case .unknown: "circle.dotted"
        }
    }

    var tint: Color {
        switch self {
        case .healthy: .green
        case .capturePaused, .watcherStopped, .noRecentCaptures: .orange
        case .setupNeeded: .blue
        case .conflict, .error, .missingBinary: .red
        case .unknown: .secondary
        }
    }
}

enum ClipboardKind: String, CaseIterable, Identifiable, Codable, Hashable, Sendable {
    case text
    case html
    case rtf
    case url
    case file
    case image
    case pdf
    case binary
    case other

    var id: String { rawValue }

    var title: String {
        switch self {
        case .text: "Text"
        case .html: "HTML"
        case .rtf: "RTF"
        case .url: "URL"
        case .file: "File"
        case .image: "Image"
        case .pdf: "PDF"
        case .binary: "Binary"
        case .other: "Other"
        }
    }
}

struct RetrievalFilterState: Equatable, Sendable {
    var hours: Int
    var appName = ""
    var bundleID = ""
    var kind: ClipboardKind?
    var hasText = false
    var hasURL = false
    var hasFile = false
    var hasImage = false
    var hasPDF = false

    var activeAdvancedFilterCount: Int {
        var count = 0
        if !appName.isEmpty { count += 1 }
        if !bundleID.isEmpty { count += 1 }
        if hasText { count += 1 }
        if hasURL { count += 1 }
        if hasFile { count += 1 }
        if hasImage { count += 1 }
        if hasPDF { count += 1 }
        return count
    }

    mutating func resetAdvanced() {
        appName = ""
        bundleID = ""
        hasText = false
        hasURL = false
        hasFile = false
        hasImage = false
        hasPDF = false
    }

    static var defaultValue: RetrievalFilterState {
        RetrievalFilterState(hours: UserDefaults.standard.clipmemDefaultHours)
    }
}
