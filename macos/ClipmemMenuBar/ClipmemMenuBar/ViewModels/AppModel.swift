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
    var clipboardHistoryRevision = 0
    var lastError: UserError?
    var actionMessage: String?
    var hotkeyMessage: String?
    var launchAtLoginEnabled = UserDefaults.standard.clipmemLaunchAtLoginEnabled
    var launchAtLoginStatus = LoginItemController.status()
    var launchAtLoginError: UserError?
    var isRefreshing = false
    var isRunningAction = false
    var updateStatus = UpdateStatus.load()
    var pendingHistorySearchQuery = ""
    var pendingHistorySearchRequestID = 0

    @ObservationIgnored private let hotKeyManager = HotKeyManager()
    @ObservationIgnored private let updateChecker = UpdateChecker()
    @ObservationIgnored private let loadRecentPreview: @MainActor () async throws -> [ClipmemItem]
    @ObservationIgnored private var pasteboardMonitor: PasteboardChangeMonitor?
    @ObservationIgnored private var recentRefreshCoordinator: RecentPreviewRefreshCoordinator?
    @ObservationIgnored private var recentPreviewRefreshedAt: Date?

    init(loadRecentPreview: (@MainActor () async throws -> [ClipmemItem])? = nil) {
        self.loadRecentPreview = loadRecentPreview ?? {
            let envelope = try await ClipmemClient(configuration: .current).recent(limit: 40, cursor: nil, filters: .defaultValue)
            return envelope.results
        }
    }

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
        case .capturePaused: "pause.circle.fill"
        case .watcherStopped: "stop.circle.fill"
        case .noRecentCaptures: "clock.arrow.circlepath"
        case .setupNeeded: "plus.circle.fill"
        case .conflict, .error: "exclamationmark.triangle.fill"
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
        startPasteboardMonitorIfNeeded()
        await checkForUpdatesIfNeeded()
    }

    func refreshAll() async {
        isRefreshing = true
        defer { isRefreshing = false }
        lastError = nil
        async let statusTask: Void = refreshStatus()
        async let settingsTask: Void = refreshSettings()
        async let recentTask: Bool = refreshRecentPreview()
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

    @discardableResult
    func refreshRecentPreview() async -> Bool {
        do {
            recentPreview = try await loadRecentPreview()
            recentPreviewRefreshedAt = Date()
            return true
        } catch {
            recentPreview = []
            return false
        }
    }

    func refreshRecentPreviewIfStale(maxAge: TimeInterval) async {
        if let recentPreviewRefreshedAt, Date().timeIntervalSince(recentPreviewRefreshedAt) < maxAge {
            return
        }
        await recentCoordinator().refreshNow()
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

    func compactDatabase() async {
        isRunningAction = true
        actionMessage = nil
        defer { isRunningAction = false }
        do {
            let report = try await client.storageCompact(dryRun: false)
            lastError = nil
            showActionMessage("Compacted database. Reclaimed \(formatBytes(report.reclaimedBytes)).")
            await refreshStatus()
        } catch {
            lastError = UserError(error)
            actionMessage = nil
        }
    }

    func optimizeImages() async {
        isRunningAction = true
        actionMessage = nil
        defer { isRunningAction = false }
        do {
            let report = try await client.storageOptimizeImages(dryRun: false, limit: nil)
            lastError = nil
            let saved = DisplayFormatters.byteCount(report.logicalSavedBytes) ?? "\(report.logicalSavedBytes) bytes"
            let reclaimed = formatBytes(report.filesystemSavedBytes)
            if let compactError = report.compactError {
                showActionMessage("Optimized \(report.compressedRows) images. Reduced image bytes by \(saved), but database compaction failed: \(compactError). Run Compact Database to retry.")
            } else if report.compactRun {
                showActionMessage("Optimized \(report.compressedRows) images. Reduced image bytes by \(saved) and reclaimed \(reclaimed) from the database.")
            } else if report.compactRecommended {
                showActionMessage("Optimized \(report.compressedRows) images. Reduced image bytes by \(saved). Run Compact Database to return freed pages to disk.")
            } else {
                showActionMessage("Optimized \(report.compressedRows) images. Reduced image bytes by \(saved).")
            }
            await refreshStatus()
        } catch {
            lastError = UserError(error)
            actionMessage = nil
        }
    }

    private func formatBytes(_ bytes: UInt64) -> String {
        let clamped = min(bytes, UInt64(Int.max))
        return DisplayFormatters.byteCount(Int(clamped)) ?? "\(bytes) bytes"
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

    func requestHistorySearch(query: String) {
        let trimmedQuery = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmedQuery.isEmpty == false else { return }
        pendingHistorySearchQuery = trimmedQuery
        pendingHistorySearchRequestID += 1
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

    private func startPasteboardMonitorIfNeeded() {
        if pasteboardMonitor != nil { return }
        let monitor = PasteboardChangeMonitor { [weak self] in
            self?.recentCoordinator().schedule()
        }
        pasteboardMonitor = monitor
        monitor.start()
    }

    private func recentCoordinator() -> RecentPreviewRefreshCoordinator {
        if let recentRefreshCoordinator {
            return recentRefreshCoordinator
        }
        let coordinator = RecentPreviewRefreshCoordinator { [weak self] in
            guard let self else { return false }
            let refreshed = await self.refreshRecentPreview()
            if refreshed {
                self.clipboardHistoryRevision += 1
            }
            return refreshed
        }
        recentRefreshCoordinator = coordinator
        return coordinator
    }
}

@MainActor
final class PasteboardChangeMonitor {
    static let defaultPollInterval: Duration = .milliseconds(250)

    private let pollInterval: Duration
    private let changeCount: @MainActor () -> Int
    private let onChange: @MainActor () -> Void
    private var task: Task<Void, Never>?
    private var lastChangeCount: Int?

    init(
        pollInterval: Duration = PasteboardChangeMonitor.defaultPollInterval,
        changeCount: @escaping @MainActor () -> Int = { NSPasteboard.general.changeCount },
        onChange: @escaping @MainActor () -> Void
    ) {
        self.pollInterval = pollInterval
        self.changeCount = changeCount
        self.onChange = onChange
    }

    deinit {
        task?.cancel()
    }

    func start() {
        guard task == nil else { return }
        lastChangeCount = changeCount()
        task = Task { [weak self] in
            while Task.isCancelled == false {
                guard let self else { return }
                try? await Task.sleep(for: self.pollInterval)
                guard Task.isCancelled == false else { return }
                self.pollOnce()
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }

    func pollOnce() {
        let currentChangeCount = changeCount()
        guard let lastChangeCount else {
            self.lastChangeCount = currentChangeCount
            return
        }
        guard currentChangeCount != lastChangeCount else { return }
        self.lastChangeCount = currentChangeCount
        onChange()
    }
}

@MainActor
final class RecentPreviewRefreshCoordinator {
    static let defaultDebounce: Duration = .milliseconds(550)

    private let debounce: Duration
    private let sleep: @MainActor (Duration) async throws -> Void
    private let refresh: @MainActor () async -> Bool
    private var pendingTask: Task<Void, Never>?
    private var isRefreshing = false
    private var needsFollowUp = false

    init(
        debounce: Duration = RecentPreviewRefreshCoordinator.defaultDebounce,
        sleep: @escaping @MainActor (Duration) async throws -> Void = { try await Task.sleep(for: $0) },
        refresh: @escaping @MainActor () async -> Bool
    ) {
        self.debounce = debounce
        self.sleep = sleep
        self.refresh = refresh
    }

    deinit {
        pendingTask?.cancel()
    }

    func schedule() {
        pendingTask?.cancel()
        pendingTask = Task { [weak self] in
            guard let self else { return }
            do {
                try await sleep(debounce)
            } catch {
                return
            }
            guard Task.isCancelled == false else { return }
            await runRefresh(queueFollowUpIfBusy: true)
        }
    }

    func refreshNow() async {
        pendingTask?.cancel()
        pendingTask = nil
        await runRefresh(queueFollowUpIfBusy: false)
    }

    private func runRefresh(queueFollowUpIfBusy: Bool) async {
        if isRefreshing {
            if queueFollowUpIfBusy {
                needsFollowUp = true
            }
            return
        }

        isRefreshing = true
        _ = await refresh()
        isRefreshing = false

        if needsFollowUp {
            needsFollowUp = false
            schedule()
        }
    }
}
