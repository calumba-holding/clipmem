# Changelog

All notable changes to `clipmem` are documented in this file. This file is the
source of truth for GitHub release notes, so every user-facing change belongs
under `Unreleased` before the next version is tagged.

The format is based on Keep a Changelog, and this project uses semantic
versioning where practical.

## Unreleased

Changes for the next release go here.

## 0.3.2 - 2026-04-20

### Changed

- Updated the README logo and macOS menu bar app icon assets to use the
  refreshed `clipmem` logo.

## 0.3.1 - 2026-04-20

### Fixed

- Fixed the local menu bar build script so it quits any existing menu bar app
  instance before launching the debug build and verifies the debug app is
  running.
- Fixed the menu bar icon asset so it uses the bundled transparent logo SVG as
  a compiled menu bar image, and added the app icon asset used by Spotlight and
  LaunchServices.

## 0.3.0 - 2026-04-20

### Added

- Added `--human` CLI output for polished terminal summaries, tables, and
  visual bars across retrieval, stats, detail, status, settings, OCR, storage,
  and archive action commands.
- Added a menu bar app manual purge flow that previews archive deletion counts
  before purging snapshots older than a chosen duration.
- Added the project logo to the README and bundled a black transparent SVG
  version for the macOS menu bar icon.

### Changed

- Added pull request CI coverage that builds and tests the macOS menu bar app
  with unsigned `xcodebuild` Debug jobs.

### Fixed

- Fixed Quick Recall's Open and Space actions so History opens focused on the
  selected snapshot instead of a generic History window.
- Fixed the macOS menu bar logo so it stays plain when healthy and shows an
  attention badge for stale, setup, error, and conflict states.
- Switched the README logo to a non-transparent PNG so it remains visible in
  GitHub dark mode.
- Fixed Homebrew formula repair for release artifacts that use a multiline
  Apple Silicon install guard.

## 0.2.13 - 2026-04-20

### Added

- Added `clipmem storage compact` for SQLite/WAL compaction and
  `clipmem storage optimize-images` for opt-in lossless WebP image
  optimization, with menu bar actions and JSON reports. Image
  optimization now compacts SQLite storage by default so freed pages
  are returned to the filesystem.

### Fixed

- Hardened Homebrew formula publishing so nested macOS and architecture guards
  are removed before tap audit runs, preventing release commits with no active
  formula URL on Linux or Intel macOS.
- Fixed menu-bar maintenance confirmations so Compact Database, Optimize
  Images, and Uninstall Service register the first button click instead of
  requiring the dropdown to be reopened.
- Fixed the menu-bar status item fallback icons so stale/setup/error states
  remain visible on macOS versions without the previous badge symbols.
- Clarified menu-bar capture status so stopped or missing watchers are shown as
  actionable service states instead of as stale clipboard activity.

## 0.2.12 - 2026-04-20

### Added

- Added database file size to `clipmem service status` text and JSON output,
  and surfaced it in the menu bar dropdown.
- Added inline search to the menu bar dropdown's recent clipboard list, with a
  shortcut into full History search when the loaded recents don't match.

### Changed

- Expanded the menu bar dropdown's recent preview and compacted its status
  summary so more clipboard history fits in the panel.
- Display clipboard capture times in the menu bar app using the Mac's local
  time zone instead of raw UTC database timestamps.

### Fixed

- Fixed generated Homebrew formula and cask files so the tap audit can validate
  release commits across Homebrew's supported OS and architecture matrix.

## 0.2.11 - 2026-04-19

### Fixed

- Fixed macOS text clipboard captures whose `public.utf16-plain-text`
  representation contained embedded NUL bytes, which could make stored text
  appear truncated or fail search even though the full text representation was
  captured.
- Added a schema repair that rebuilds stored snapshot text projections from
  captured item representations so affected existing captures become
  searchable after upgrade.

## 0.2.10 - 2026-04-19

### Changed

- Updated README and GitHub repository description to mention opt-in local OCR
  for copied images.

### Fixed

- Added the missing menu bar app Settings toggle for enabling or disabling
  local OCR for copied images.

## 0.2.9 - 2026-04-19

### Added

- Added opt-in local OCR for copied image snapshots on macOS using Apple
  Vision, including background OCR for new captures, backfill with
  `clipmem ocr run`, and queue reporting with `clipmem ocr status`.
- Added OCR settings with `clipmem settings ocr on|off`; OCR is disabled by
  default.
- Added OCR text/status fields to flattened JSON output and indexed completed
  OCR text for `search`, `recall`, `recent`, `timeline`, and `get`.

### Changed

- Bumped the JSON output schema version to `2` because flattened retrieval rows
  now include OCR fields.

## 0.2.8 - 2026-04-19

### Added

- Added `clipmem stats` with text and JSON output for archive aggregates,
  app/activity leaderboards, content mix, dedupe ratio, and shared retrieval
  filters.

## 0.2.7 - 2026-04-19

### Added

- Added a full documentation set under `docs/`, including installation,
  getting started, command reference, agent integration, archive management,
  output formats, privacy, architecture, menu bar app, and troubleshooting
  guides.
- Added menu bar app update checks against the latest stable GitHub release.
- Added update availability UI in the menu bar panel and settings window, with
  actions to copy the Homebrew upgrade command or open the release page.

### Changed

- Reduced `README.md` to a concise project overview that points readers to the
  deeper documentation pages.

## 0.2.6 - 2026-04-19

### Fixed

- Fixed `clipmem setup` recovery when a previously disabled direct LaunchAgent
  caused `launchctl bootstrap` to fail with status 5.

## 0.2.5 - 2026-04-19

### Added

- Added menu bar app screenshots to the README and release documentation.

### Fixed

- Fixed release app signing for notarization by disabling injected base
  entitlements and adding timestamped signing flags.

### Changed

- Updated GitHub Actions checkout steps to `actions/checkout@v6`.

## 0.2.4 - 2026-04-19

### Changed

- Added `Sendable` conformance to menu bar app model, client, and request types
  used across Swift concurrency boundaries.
- Bumped the menu bar app marketing version to `0.2.4`.

## 0.2.3 - 2026-04-19

### Changed

- Moved menu bar app release jobs to the `macos-15` GitHub Actions runner.
- Bumped the menu bar app marketing version to `0.2.3`.

## 0.2.2 - 2026-04-19

### Added

- Added a native SwiftUI macOS menu bar app with history browsing, quick recall,
  diagnostics, settings, launch-at-login support, and an Option-Shift-V quick
  recall hotkey.
- Added the Homebrew cask release path for installing the CLI and menu bar app
  together.
- Added clipboard restore, forget, purge, and persistent settings commands.
- Added pause, retention, ignored app, ignored bundle ID, and API-key filtering
  controls for capture policy.
- Added menu bar app tests, fixtures, command construction checks, and decoding
  coverage.
- Added clipboard-memory skill eval fixtures and improved setup checks.

### Changed

- Hardened release automation with a local `cargo-dist` installer, audited
  installer setup, trusted crate publishing, and hand-maintained workflow
  updates.
- Updated the service setup flow so Homebrew installs use direct LaunchAgent
  management unless a Homebrew service stanza is available.
- Expanded README and release documentation for the menu bar app, policy
  controls, and the split Homebrew formula and cask install surfaces.

### Fixed

- Hardened export destination handling, including explicit overwrite behavior.
- Fixed service binary path handling to avoid PATH poisoning.
- Fixed menu bar app setup feedback, window activation, command construction,
  error handling, filter handling, and review findings.

## 0.2.1 - 2026-04-17

### Changed

- Tightened TOON skim output for agent-facing retrieval flows.
- Reduced retrieval latency across `recent`, `search`, `recall`, `timeline`,
  and startup by moving read hot paths onto maintained snapshot-level caches
  instead of rebuilding archive-wide aggregates at query time.
- Accelerated filtered FTS searches by avoiding global event materialization
  and using cheaper snapshot-level filtering on common app and bundle-id paths.
- Accelerated literal search with trigram-backed candidate narrowing and
  dedicated fast paths for punctuation-heavy text and file-path lookups.
- Reduced healthy-open startup cost by skipping cache rebuilds when an existing
  database is already at the current schema version.

### Performance

- Improved `recent` on the large retrieval harness from roughly `98ms` at
  `v0.2.0` to single-digit milliseconds.
- Improved app-filtered FTS search from roughly `940ms` at its original hot
  spot to about `15ms`.
- Improved literal path lookups from tens of milliseconds to low single-digit
  milliseconds.
- Improved opening an existing healthy archive from roughly `118ms` to about
  `2-3ms`.

### Notes

- Existing databases continue to migrate forward automatically on open.
- Release automation is driven by a `v0.2.1` tag and validates that the tag
  matches `Cargo.toml`.

## 0.2.0 - 2026-04-17

### Added

- Added `clipmem setup` as the canonical onboarding command for seeding the
  archive and starting background capture.
- Added service management commands for status, start, stop, and uninstall.
- Added LaunchAgent status reporting and setup diagnostics for agent skill
  check scripts.

### Changed

- Productized the background capture flow so Homebrew, Cargo, and source
  installs share the same setup behavior.
- Updated LaunchAgent install and uninstall scripts to delegate to the CLI
  service workflow.
- Refreshed agent skill guidance for the new setup and service commands.

## 0.1.2 - 2026-04-17

### Added

- Added `skills/clipboard-memory/` as the canonical cross-agent skill package.

### Changed

- Renamed the OpenClaw skill package path from `clipboard_memory` to
  `clipboard-memory`.
- Updated OpenClaw install, uninstall, doctor, README, and tests to use the
  hyphenated skill package name.

## 0.1.1 - 2026-04-17

### Added

- Added `clipmem recall` for ranked, agent-facing clipboard retrieval.
- Added `clipmem timeline` for chronological capture-event retrieval.
- Added flattened text projections across retrieval output so agents can read
  clipboard content without walking raw representation data.
- Added portable and OpenClaw-native clipboard-memory skill packages with
  command reference, examples, JSON schema, setup checks, and troubleshooting
  docs.
- Added parity tests for packaged skill content.

### Changed

- Improved clipboard query ranking and OpenClaw skill packaging.
- Polished CLI help, exit codes, and stderr handling.
- Rewrote the README around the current CLI and agent workflow.

## 0.1.0 - 2026-04-17

### Added

- Added the initial macOS clipboard memory CLI backed by SQLite.
- Added clipboard capture from `NSPasteboard`, snapshot deduplication, capture
  events, raw representation storage, and frontmost-app source hints.
- Added searchable text projections with SQLite FTS5 and literal search
  fallback.
- Added commands to capture once, watch the clipboard, search history, list
  recent snapshots, inspect snapshots, export raw representations, and run
  database diagnostics.
- Added LaunchAgent install and uninstall scripts for background capture.
- Added the first OpenClaw skill package and installer script.
- Added Homebrew, crates.io, and GitHub release automation.

### Fixed

- Hardened search fallback escaping and UTF-16 decoding.
- Hardened archive storage, model boundaries, CLI rendering, watcher setup, and
  installer flows before the first public release.
- Hardened crates.io publishing preflight checks.
