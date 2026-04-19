import OSLog

enum AppLoggers {
    static let menuBar = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "MenuBar")
    static let windowing = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "Windowing")
    static let commands = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "Commands")
    static let search = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "Search")
    static let service = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "Service")
    static let hotkey = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "Hotkey")
    static let export = Logger(subsystem: "io.openclaw.clipmem.menubar", category: "Export")
}
