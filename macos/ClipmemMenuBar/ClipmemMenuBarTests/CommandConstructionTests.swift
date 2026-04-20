import Testing
@testable import ClipmemMenuBar

struct CommandConstructionTests {
    @Test func databaseOverrideIsPrepended() {
        let command = ClipmemCommand.recent(limit: 25, cursor: "abc", filters: .defaultValue)
            .withDatabase("/tmp/clipmem.sqlite3")

        #expect(command.arguments.prefix(2) == ["--db", "/tmp/clipmem.sqlite3"])
        #expect(command.arguments.contains("recent"))
        #expect(command.arguments.contains("--format"))
        #expect(command.arguments.contains("json"))
        #expect(command.arguments.contains("--cursor"))
    }

    @Test func actionCommandsRequestJSON() {
        #expect(ClipmemCommand.restore(snapshotID: 42).arguments == ["restore", "42", "--format", "json"])
        #expect(ClipmemCommand.forget(snapshotID: 42).arguments == ["forget", "42", "--format", "json"])
        #expect(ClipmemCommand.purge(olderThan: "30d", dryRun: true).arguments == ["purge", "--older-than", "30d", "--format", "json", "--dry-run"])
        #expect(ClipmemCommand.purge(olderThan: "30d", dryRun: false).arguments == ["purge", "--older-than", "30d", "--format", "json"])
        #expect(ClipmemCommand.export(snapshotID: 42, itemIndex: 0, uti: "public.png", destination: "/tmp/a.png", force: true).arguments.contains("--force"))
        #expect(ClipmemCommand.settingsOCR(true).arguments == ["settings", "ocr", "on"])
        #expect(ClipmemCommand.settingsOCR(false).arguments == ["settings", "ocr", "off"])
        #expect(ClipmemCommand.storageCompact(dryRun: false).arguments == ["storage", "compact", "--format", "json"])
        #expect(ClipmemCommand.storageCompact(dryRun: true).arguments.contains("--dry-run"))
        #expect(ClipmemCommand.storageOptimizeImages(dryRun: false, limit: 50).arguments == ["storage", "optimize-images", "--format", "json", "--limit", "50"])
        #expect(ClipmemCommand.storageOptimizeImages(dryRun: true, limit: nil).arguments.contains("--dry-run"))
    }

    @Test func filtersAppendExpectedFlags() {
        var filters = RetrievalFilterState(hours: 12)
        filters.appName = "Safari"
        filters.bundleID = "com.apple.Safari"
        filters.kind = .url
        filters.hasURL = true

        let command = ClipmemCommand.search(query: "example.com", limit: 10, cursor: nil, filters: filters)

        #expect(command.arguments.contains("--app"))
        #expect(command.arguments.contains("Safari"))
        #expect(command.arguments.contains("--bundle-id"))
        #expect(command.arguments.contains("com.apple.Safari"))
        #expect(command.arguments.contains("--kind"))
        #expect(command.arguments.contains("url"))
        #expect(command.arguments.contains("--has-url"))
    }

    @Test func searchQueryUsesOptionTerminator() throws {
        var filters = RetrievalFilterState(hours: 12)
        filters.appName = "Terminal"

        for query in ["--help", "-foo", "--format"] {
            let arguments = ClipmemCommand.search(query: query, limit: 10, cursor: "next", filters: filters).arguments
            let terminatorIndex = try #require(arguments.firstIndex(of: "--"))

            #expect(arguments[terminatorIndex + 1] == query)
            #expect(arguments.firstIndex(of: "--limit")! < terminatorIndex)
            #expect(arguments.firstIndex(of: "--cursor")! < terminatorIndex)
            #expect(arguments.firstIndex(of: "--app")! < terminatorIndex)
        }
    }

    @Test func recallQueryUsesOptionTerminatorButRecentRecallDoesNot() throws {
        let queried = ClipmemCommand.recall(query: "--help", limit: 12, filters: .defaultValue).arguments
        let terminatorIndex = try #require(queried.firstIndex(of: "--"))
        let queryIndex = try #require(queried.firstIndex(of: "--help"))

        #expect(queryIndex == terminatorIndex + 1)
        #expect(queried.firstIndex(of: "--limit")! < terminatorIndex)

        let recent = ClipmemCommand.recall(query: nil, limit: 12, filters: .defaultValue).arguments
        #expect(recent.contains("--prefer-recent"))
        #expect(!recent.contains("--"))
    }
}
