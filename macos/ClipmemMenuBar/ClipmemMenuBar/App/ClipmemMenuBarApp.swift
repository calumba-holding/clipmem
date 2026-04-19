import AppKit
import SwiftUI

@main
struct ClipmemMenuBarApp: App {
    @Environment(\.openWindow) private var openWindow
    @AppStorage(PreferenceKey.hotkeyEnabled) private var hotkeyEnabled = true
    @State private var appModel = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuBarPanelView(model: appModel)
                .frame(width: 380, height: 520)
        } label: {
            Label("clipmem", systemImage: appModel.menuBarSymbol)
                .task {
                    await appModel.start()
                    configureHotkey()
                }
                .onChange(of: hotkeyEnabled) {
                    configureHotkey()
                }
        }
        .menuBarExtraStyle(.window)

        WindowGroup("History", id: WindowID.history.rawValue) {
            HistoryWindowView(appModel: appModel)
                .frame(minWidth: 960, minHeight: 620)
        }
        .commands {
            InspectorCommands()
        }
        .keyboardShortcut("h", modifiers: [.command, .shift])

        Window("Quick Recall", id: WindowID.quickRecall.rawValue) {
            QuickRecallWindowView(appModel: appModel)
                .frame(width: 720, height: 520)
        }
        .keyboardShortcut("v", modifiers: [.command, .shift])

        Settings {
            ClipmemSettingsView(appModel: appModel)
                .frame(width: 560, height: 520)
        }
    }

    @MainActor
    private func configureHotkey() {
        appModel.configureHotkey(enabled: hotkeyEnabled) {
            openWindow(id: WindowID.quickRecall.rawValue)
            NSApp.activate(ignoringOtherApps: true)
        }
    }
}
