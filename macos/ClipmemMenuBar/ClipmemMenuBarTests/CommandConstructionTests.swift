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
        #expect(ClipmemCommand.purge(olderThan: "30d", dryRun: true).arguments.contains("--dry-run"))
        #expect(ClipmemCommand.export(snapshotID: 42, itemIndex: 0, uti: "public.png", destination: "/tmp/a.png", force: true).arguments.contains("--force"))
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
}
