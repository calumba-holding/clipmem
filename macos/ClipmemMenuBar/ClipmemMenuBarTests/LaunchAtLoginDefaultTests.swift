import Foundation
import Testing
@testable import ClipmemMenuBar

struct LaunchAtLoginDefaultTests {
    @Test func activeHostedSuiteIsRecognizedAsTestHost() {
        #expect(AppStartupMode.current == .testHost)
    }

    @Test func xctestHostEnvironmentDisablesApplicationSideEffects() {
        let mode = AppStartupMode.resolve(
            environment: ["XCTestConfigurationFilePath": "/tmp/ClipmemMenuBarTests.xctestconfiguration"],
            xctestRuntimeLoaded: false
        )

        #expect(mode == .testHost)
        #expect(mode.allowsApplicationSideEffects == false)
    }

    @Test func productionEnvironmentKeepsApplicationStartupEnabled() {
        let mode = AppStartupMode.resolve(environment: [:], xctestRuntimeLoaded: false)

        #expect(mode == .production)
        #expect(mode.allowsApplicationSideEffects)
    }

    @Test @MainActor
    func testHostAppModelStartDoesNotRunAnyArchiveLoad() async {
        let model = AppModel(startupMode: .testHost) {
            Issue.record("Test-host startup must not attempt an archive read.")
            return []
        }

        let started = await model.start()

        #expect(started == false)
        #expect(model.serviceStatus == nil)
        #expect(model.settingsReport == nil)
        #expect(model.recentPreview.isEmpty)
    }

    @Test func preservesExistingCliPreferenceWhenConfiguredMarkerIsMissing() throws {
        let defaults = try temporaryDefaults()
        defaults.set(false, forKey: PreferenceKey.launchAtLoginEnabled)

        let action = LaunchAtLoginDefaultConfigurator.configureIfNeeded(
            defaults: defaults,
            defaultEnabled: true
        )

        #expect(action == .refreshFromDefaults)
        #expect(defaults.bool(forKey: PreferenceKey.launchAtLoginEnabled) == false)
        #expect(defaults.bool(forKey: PreferenceKey.didConfigureLaunchAtLogin) == true)
    }

    @Test func appliesDisabledBundleDefaultWithoutLoginItemMutation() throws {
        let defaults = try temporaryDefaults()

        let action = LaunchAtLoginDefaultConfigurator.configureIfNeeded(
            defaults: defaults,
            defaultEnabled: false
        )

        #expect(action == .refreshFromDefaults)
        #expect(defaults.bool(forKey: PreferenceKey.launchAtLoginEnabled) == false)
        #expect(defaults.bool(forKey: PreferenceKey.didConfigureLaunchAtLogin) == true)
    }

    @Test func requestsLoginItemEnableWhenDefaultIsEnabledAndNoPreferenceExists() throws {
        let defaults = try temporaryDefaults()

        let action = LaunchAtLoginDefaultConfigurator.configureIfNeeded(
            defaults: defaults,
            defaultEnabled: true
        )

        #expect(action == .enableLoginItem)
        #expect(defaults.object(forKey: PreferenceKey.launchAtLoginEnabled) == nil)
        #expect(defaults.bool(forKey: PreferenceKey.didConfigureLaunchAtLogin) == false)
    }

    private func temporaryDefaults() throws -> UserDefaults {
        let suiteName = "clipmem-tests-\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suiteName))
        defaults.removePersistentDomain(forName: suiteName)
        return defaults
    }
}
