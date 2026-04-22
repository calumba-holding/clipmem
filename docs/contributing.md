# Contributing

clipmem is a Rust CLI with a native SwiftUI macOS frontend. The
codebase is structured to make extension straightforward — export
commands, embeddings, OCR, richer source-app heuristics, and fuller
HTML parsing all fit the existing shape.

## Project layout

```
src/                           Rust source
  main.rs                      CLI entry point
  cli.rs                       Argument parsing (clap) and command dispatch
  cli/commands.rs              Command implementations
  cli/output.rs                Output formatting (text, JSON, JSONL, MD, TOON)
  cli/service.rs               Service and setup management
  cli/db_path.rs               Database path resolution
  db.rs                        Database connection and retrieval logic
  db/schema.sql                SQLite schema (tables, indexes, triggers, FTS5)
  db/store.rs                  Insertion and storage logic
  db/read.rs                   Query and search logic
  capture.rs                   Public capture facade
  model/                       Data structures
    clipboard.rs               ClipboardSnapshot, ClipboardItem, ClipboardRepresentation
    archive.rs                 SearchHit, TimelineEvent, CaptureEvent, SnapshotDetails
    kinds.rs                   ClipboardKind, SnapshotKind enums
    text_projection.rs         FlattenedTextProjection, TextFragment
    builders.rs                Snapshot, item, and representation builders
  platform/
    macos.rs                   NSPasteboard capture, restore, app detection via objc2
    unsupported.rs             Stubs for non-macOS platforms
  app.rs                       Watch state and capture line formatting
  sensitive.rs                 API key filter heuristics
  archive.rs                   Public archive facade

macos/ClipmemMenuBar/          Native SwiftUI menu bar app and Swift Testing suite

tests/
  cli_commands.rs              CLI integration tests
  database_roundtrip.rs        Schema and serialization tests
  skill_parity.rs              OpenClaw skill verification

skills/clipboard-memory/       Canonical public skill content
extras/openclaw/               OpenClaw-native skill package
extras/agent-skills/           Portable skill package mirror
extras/launchd/                LaunchAgent plist template

scripts/
  build_and_run_menubar.sh     Build Rust + SwiftUI app and launch
  clawhub_skill_sync.py        Check and publish the clipboard-memory skill on ClawHub
  install_launchagent.sh       Compatibility wrapper for clipmem setup
  uninstall_launchagent.sh     Compatibility wrapper for clipmem service uninstall
  install_openclaw_skill.sh    Compatibility wrapper for clipmem agents openclaw install-skill
```

## Platform gating

Clipboard capture is gated behind `cfg(target_os = "macos")`. The
database, search, and test layers compile on other platforms for
development, but actual capture and restore only work on macOS.

## Build and test

Build the Rust CLI (requires Rust 1.88+):

```bash
cargo build --release
```

Run the local validation suite before opening a PR:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
python3 scripts/check_file_lengths.py
cargo test
```

The file-length lint checks tracked Rust, Swift, and Rust test files. The
default ceiling is 800 lines, with explicit per-file overrides in
`file-length-limits.json` for the current large files. Treat those overrides as
a ratchet, not a free pass.

The CI workflow hard-fails on formatting, Clippy, the file-length lint, tests,
and crate packaging checks, so running the same commands locally keeps review
cycles short.

The CLI integration tests in `tests/cli_commands.rs` exercise every command,
flag combination, filter, and output format.

## Build and test the menu bar app

Build and launch the development app from the repo root:

```bash
scripts/build_and_run_menubar.sh
```

Or build and test directly with `xcodebuild`:

```bash
xcodebuild -project macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj \
  -scheme ClipmemMenuBar -configuration Debug \
  -derivedDataPath macos/ClipmemMenuBar/DerivedData build

xcodebuild -project macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj \
  -scheme ClipmemMenuBar -configuration Debug \
  -derivedDataPath macos/ClipmemMenuBar/DerivedData test
```

## Release workflow

See [RELEASING.md](../RELEASING.md) for the full release workflow,
including tag-driven builds, GitHub Actions CI, Homebrew formula
updates, and crates.io publishing.

## Next steps

- [Architecture](architecture.md) — understand the capture model,
  storage design, and search strategy
- [Command reference](command-reference.md) — exhaustive flag-level
  reference
