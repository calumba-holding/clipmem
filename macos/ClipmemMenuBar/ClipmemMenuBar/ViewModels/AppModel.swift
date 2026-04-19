import AppKit
import Foundation
import Observation

@MainActor
@Observable
final class AppModel {
    var serviceStatus: ServiceStatusReport?
    var doctorReport: DoctorReport?
    var settingsReport: SettingsReport?
    var recentPreview: [ClipmemItem] = []
    var lastErrorMessage: String?
    var hotkeyMessage: String?
    var isRefreshing = false

    @ObservationIgnored private let hotKeyManager = HotKeyManager()

    var healthState: HealthState {
        if client.resolvedBinaryPath() == nil {
            return .missingBinary
        }
        return serviceStatus?.health ?? .unknown
    }

    var menuBarSymbol: String {
        switch healthState {
        case .healthy: "paperclip.circle.fill"
        case .stale: "paperclip.badge.clock"
        case .setupNeeded: "paperclip.badge.plus"
        case .conflict, .error: "paperclip.badge.exclamationmark"
        case .missingBinary: "questionmark.folder"
        case .unknown: "paperclip"
        }
    }

    var client: ClipmemClient {
        ClipmemClient(configuration: .current)
    }

    func start() async {
        await installSelfIgnoreIfNeeded()
        await refreshAll()
    }

    func refreshAll() async {
        isRefreshing = true
        defer { isRefreshing = false }
        lastErrorMessage = nil
        async let statusTask: Void = refreshStatus()
        async let settingsTask: Void = refreshSettings()
        async let recentTask: Void = refreshRecentPreview()
        _ = await (statusTask, settingsTask, recentTask)
    }

    func refreshStatus() async {
        do {
            serviceStatus = try await client.serviceStatus()
        } catch {
            serviceStatus = nil
            lastErrorMessage = error.localizedDescription
        }
    }

    func refreshDoctor() async {
        do {
            doctorReport = try await client.doctor()
        } catch {
            doctorReport = nil
            lastErrorMessage = error.localizedDescription
        }
    }

    func refreshSettings() async {
        do {
            settingsReport = try await client.settings()
        } catch {
            settingsReport = nil
        }
    }

    func refreshRecentPreview() async {
        do {
            let envelope = try await client.recent(limit: 8, cursor: nil, filters: .defaultValue)
            recentPreview = envelope.results
        } catch {
            recentPreview = []
        }
    }

    func runSetup() async {
        await runAction(.setup())
        await refreshAll()
    }

    func serviceAction(_ action: String) async {
        await runAction(.service(action))
        await refreshAll()
    }

    func runAction(_ command: ClipmemCommand) async {
        do {
            try await client.runAction(command)
            lastErrorMessage = nil
        } catch {
            lastErrorMessage = error.localizedDescription
        }
    }

    func restore(_ item: ClipmemItem) async {
        do {
            _ = try await client.restore(snapshotID: item.snapshotId)
            lastErrorMessage = nil
            await refreshRecentPreview()
        } catch {
            lastErrorMessage = error.localizedDescription
        }
    }

    func forget(_ item: ClipmemItem) async {
        do {
            _ = try await client.forget(snapshotID: item.snapshotId)
            recentPreview.removeAll { $0.snapshotId == item.snapshotId }
            lastErrorMessage = nil
        } catch {
            lastErrorMessage = error.localizedDescription
        }
    }

    func openLogsFolder() {
        guard let path = serviceStatus?.logPaths.first else { return }
        let url = URL(fileURLWithPath: path).deletingLastPathComponent()
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    func configureHotkey(enabled: Bool, openQuickRecall: @escaping @MainActor () -> Void) {
        if enabled {
            hotkeyMessage = hotKeyManager.registerDefault(action: openQuickRecall)
        } else {
            unregisterHotkey()
        }
    }

    func unregisterHotkey() {
        hotKeyManager.unregister()
        hotkeyMessage = nil
    }

    private func installSelfIgnoreIfNeeded() async {
        let defaults = UserDefaults.standard
        guard defaults.bool(forKey: PreferenceKey.didInstallSelfIgnore) == false else { return }
        do {
            try await client.runAction(.settingsIgnoreAdd("io.openclaw.clipmem.menubar"))
            defaults.set(true, forKey: PreferenceKey.didInstallSelfIgnore)
        } catch {
            AppLoggers.service.info("Self ignore setup was skipped or failed")
        }
    }
}
