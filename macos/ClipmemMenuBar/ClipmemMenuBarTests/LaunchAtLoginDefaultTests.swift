import Foundation
import Testing
@testable import ClipmemMenuBar

struct LaunchAtLoginDefaultTests {
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
