import AppKit
import Foundation
import Observation
import SwiftUI

@MainActor
@Observable
final class AppModel {
    var serviceStatus: ServiceStatusReport?
    var doctorReport: DoctorReport?
    var settingsReport: SettingsReport?
    var recentPreview: [ClipmemItem] = []
    var lastError: UserError?
    var actionMessage: String?
    var hotkeyMessage: String?
    var launchAtLoginEnabled = UserDefaults.standard.clipmemLaunchAtLoginEnabled
    var launchAtLoginStatus = LoginItemController.status()
    var launchAtLoginError: UserError?
    var isRefreshing = false
    var isRunningAction = false
    var updateStatus = UpdateStatus.load()

    @ObservationIgnored private let hotKeyManager = HotKeyManager()
    @ObservationIgnored private let updateChecker = UpdateChecker()

    // Keep backward compatibility for views that check the string directly
    var lastErrorMessage: String? { lastError?.message }

    var healthState: HealthState {
        if client.resolvedBinaryPath() == nil {
            return .missingBinary
        }
        return serviceStatus?.health ?? .unknown
    }

    var menuBarSymbol: String {
        switch healthState {
        case .healthy: updateStatus.isUpdateAvailable ? "arrow.down.circle.fill" : "paperclip.circle.fill"
        case .stale: "paperclip.badge.clock"
        case .setupNeeded: "paperclip.badge.plus"
        case .conflict, .error: "paperclip.badge.exclamationmark"
        case .missingBinary: "questionmark.folder"
        case .unknown: updateStatus.isUpdateAvailable ? "arrow.down.circle" : "paperclip"
        }
    }

    var client: ClipmemClient {
        ClipmemClient(configuration: .current)
    }

    func start() async {
        configureDefaultLaunchAtLoginIfNeeded()
        await installSelfIgnoreIfNeeded()
        await refreshAll()
        await checkForUpdatesIfNeeded()
    }

    func refreshAll() async {
        isRefreshing = true
        defer { isRefreshing = false }
        lastError = nil
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
            lastError = UserError(error)
        }
    }

    func refreshDoctor() async {
        do {
            doctorReport = try await client.doctor()
        } catch {
            doctorReport = nil
            lastError = UserError(error)
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
        if await runAction(.setup(), successMessage: "Setup completed.") {
            await refreshAll()
        }
    }

    func serviceAction(_ action: String) async {
        if await runAction(.service(action), successMessage: "Service \(action) completed.") {
            await refreshAll()
        }
    }

    @discardableResult
    func runAction(_ command: ClipmemCommand, successMessage: String? = nil) async -> Bool {
        isRunningAction = true
        actionMessage = nil
        defer { isRunningAction = false }
        do {
            try await client.runAction(command)
            lastError = nil
            showActionMessage(successMessage)
            return true
        } catch {
            lastError = UserError(error)
            actionMessage = nil
            return false
        }
    }

    func restore(_ item: ClipmemItem) async {
        do {
            _ = try await client.restore(snapshotID: item.snapshotId)
            lastError = nil
            showActionMessage("Restored to clipboard")
            await refreshRecentPreview()
        } catch {
            lastError = UserError(error)
        }
    }

    @discardableResult
    func forget(_ item: ClipmemItem) async -> Bool {
        do {
            _ = try await client.forget(snapshotID: item.snapshotId)
            recentPreview.removeAll { $0.snapshotId == item.snapshotId }
            lastError = nil
            return true
        } catch {
            lastError = UserError(error)
            return false
        }
    }

    func openLogsFolder() {
        guard let path = serviceStatus?.logPaths.first else { return }
        let url = URL(fileURLWithPath: path).deletingLastPathComponent()
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    func checkForUpdatesIfNeeded() async {
        guard updateStatus.shouldCheck() else { return }
        await checkForUpdates(force: false, manual: false)
    }

    func checkForUpdates(force: Bool = true, manual: Bool = true) async {
        if updateStatus.isChecking {
            return
        }
        if force == false, updateStatus.shouldCheck() == false {
            return
        }

        updateStatus.beginCheck(manual: manual)
        do {
            let result = try await updateChecker.latestStableRelease()
            updateStatus.applySuccess(result)
        } catch {
            updateStatus.applyFailure(error, manual: manual)
        }
    }

    func copyUpgradeCommand() {
        PasteboardActions.copyPlainText(UpdateChecker.homebrewUpgradeCommand)
        showActionMessage("Upgrade command copied")
    }

    func openUpdateRelease() {
        guard let releaseURL = updateStatus.releaseURL else { return }
        NSWorkspace.shared.open(releaseURL)
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

    func setLaunchAtLoginEnabled(_ enabled: Bool) {
        UserDefaults.standard.set(true, forKey: PreferenceKey.didConfigureLaunchAtLogin)
        do {
            try LoginItemController.setEnabled(enabled)
            UserDefaults.standard.set(enabled, forKey: PreferenceKey.launchAtLoginEnabled)
            launchAtLoginEnabled = enabled
            launchAtLoginStatus = LoginItemController.status()
            launchAtLoginError = nil
        } catch {
            launchAtLoginStatus = LoginItemController.status()
            launchAtLoginEnabled = launchAtLoginStatus == .enabled
            UserDefaults.standard.set(launchAtLoginEnabled, forKey: PreferenceKey.launchAtLoginEnabled)
            launchAtLoginError = UserError(
                message: "Could not update launch at login.",
                recovery: error.localizedDescription
            )
        }
    }

    // MARK: - Private

    private func showActionMessage(_ message: String?) {
        actionMessage = message
        if let message {
            Task {
                try? await Task.sleep(for: .seconds(2.5))
                if self.actionMessage == message {
                    withAnimation { self.actionMessage = nil }
                }
            }
        }
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

    private func configureDefaultLaunchAtLoginIfNeeded() {
        let defaults = UserDefaults.standard
        if defaults.bool(forKey: PreferenceKey.didConfigureLaunchAtLogin) == false {
            let defaultEnabled = LoginItemController.bundleDefaultEnabled
            if defaultEnabled {
                setLaunchAtLoginEnabled(true)
                return
            }
            defaults.set(defaultEnabled, forKey: PreferenceKey.launchAtLoginEnabled)
            defaults.set(true, forKey: PreferenceKey.didConfigureLaunchAtLogin)
            launchAtLoginEnabled = defaultEnabled
        }
        launchAtLoginEnabled = defaults.clipmemLaunchAtLoginEnabled
        launchAtLoginStatus = LoginItemController.status()
    }
}
