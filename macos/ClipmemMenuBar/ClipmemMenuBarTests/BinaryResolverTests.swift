import Testing
@testable import ClipmemMenuBar

struct BinaryResolverTests {
    @Test func resolutionPrefersEnvironmentBeforeOverrideAndRepo() {
        let resolver = BinaryResolver(
            environment: ["CLIPMEM_BINARY_PATH": "/env/clipmem", "HOME": "/home/test"],
            userOverride: "/override/clipmem",
            repoRoot: "/repo",
            fileExists: { $0 == "/env/clipmem" || $0 == "/override/clipmem" }
        )

        #expect(resolver.resolve() == "/env/clipmem")
    }

    @Test func resolutionFallsThroughToHomeCandidates() {
        let resolver = BinaryResolver(
            environment: ["HOME": "/Users/test"],
            userOverride: nil,
            repoRoot: nil,
            fileExists: { $0 == "/Users/test/.cargo/bin/clipmem" }
        )

        #expect(resolver.resolve() == "/Users/test/.cargo/bin/clipmem")
    }

    @Test func candidatesAreDeduplicated() {
        let resolver = BinaryResolver(
            environment: ["CLIPMEM_BINARY_PATH": "/same/clipmem", "HOME": "/Users/test"],
            userOverride: "/same/clipmem",
            repoRoot: nil,
            fileExists: { _ in false }
        )

        #expect(resolver.candidates().filter { $0 == "/same/clipmem" }.count == 1)
    }
}
