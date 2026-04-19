# Changelog

## 0.2.6 - 2026-04-19

### Fixed

- Fixed `clipmem setup` recovery when a previously disabled direct LaunchAgent caused `launchctl bootstrap` to fail with status 5.

## 0.2.5 - 2026-04-19

### Added

- Added the native macOS menu bar app release path, including Developer ID signing, notarization, stapling, and Homebrew cask publishing.
- Added separate Homebrew install surfaces for the CLI-only formula and the CLI plus menu bar app cask.
- Added launch-at-login support and settings controls for the menu bar app.

### Fixed

- Hardened the menu bar app command construction for hyphen-prefixed recall and search queries.
- Kept failed forget operations visible in the UI instead of removing rows before the CLI confirms deletion.
- Disabled plain-text copy actions when no text is available, avoiding accidental clipboard clearing.

## 0.2.1 - 2026-04-17

### Changed

- Tightened TOON skim output for agent-facing retrieval flows.
- Reduced retrieval latency across recent, search, recall, timeline, and startup by moving read hot paths onto maintained snapshot-level caches instead of rebuilding archive-wide aggregates at query time.
- Accelerated filtered FTS searches by avoiding global event materialization and using cheaper snapshot-level filtering on common app and bundle-id paths.
- Accelerated literal search with trigram-backed candidate narrowing and dedicated fast paths for punctuation-heavy text and file-path lookups.
- Reduced healthy-open startup cost by skipping cache rebuilds when an existing database is already at the current schema version.

### Performance

- `recent` on the large retrieval harness improved from roughly `98ms` at `v0.2.0` to single-digit milliseconds.
- App-filtered FTS search improved from roughly `940ms` at its original hot spot to about `15ms`.
- Literal path lookups improved from tens of milliseconds to low single-digit milliseconds.
- Opening an existing healthy archive improved from roughly `118ms` to about `2-3ms`.

### Notes

- Existing databases continue to migrate forward automatically on open.
- Release automation is driven by a `v0.2.1` tag and validates that the tag matches `Cargo.toml`.
