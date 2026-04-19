import Foundation

struct BinaryResolver: Sendable {
    typealias Exists = @Sendable (String) -> Bool

    var environment: [String: String]
    var userOverride: String?
    var repoRoot: String?
    var fileExists: Exists

    init(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        userOverride: String? = UserDefaults.standard.string(forKey: PreferenceKey.binaryPathOverride),
        repoRoot: String? = BinaryResolver.defaultRepoRoot(),
        fileExists: @escaping Exists = { FileManager.default.isExecutableFile(atPath: $0) }
    ) {
        self.environment = environment
        self.userOverride = userOverride
        self.repoRoot = repoRoot
        self.fileExists = fileExists
    }

    func resolve() -> String? {
        candidates().first { fileExists($0) }
    }

    func candidates() -> [String] {
        var values: [String?] = [
            clean(environment["CLIPMEM_BINARY_PATH"]),
            clean(userOverride)
        ]

        if let repoRoot = clean(repoRoot) {
            values.append("\(repoRoot)/target/debug/clipmem")
            values.append("\(repoRoot)/target/release/clipmem")
        }

        values.append(contentsOf: [
            "/opt/homebrew/bin/clipmem",
            "/usr/local/bin/clipmem",
            expandHome("~/.cargo/bin/clipmem"),
            expandHome("~/.local/bin/clipmem")
        ])

        var seen = Set<String>()
        return values.compactMap { value in
            guard let value, seen.insert(value).inserted else { return nil }
            return value
        }
    }

    static func defaultRepoRoot() -> String? {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<6 {
            url.deleteLastPathComponent()
        }
        return url.path
    }

    private func clean(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : expandHome(trimmed)
    }

    private func expandHome(_ path: String) -> String {
        guard path.hasPrefix("~/") else { return path }
        let home = environment["HOME"] ?? NSHomeDirectory()
        return URL(fileURLWithPath: home).appendingPathComponent(String(path.dropFirst(2))).path
    }
}
