# clipmem

`clipmem` is a small Rust CLI for macOS that archives clipboard changes into SQLite, indexes the searchable text with FTS5, and gives you a local command that OpenClaw can call whenever it needs to recall something you copied.

It captures the current `NSPasteboard` contents, stores every representation it can read for every pasteboard item, deduplicates identical clipboard snapshots by SHA-256 fingerprint, and records each observation as its own event.

## What it stores

For each clipboard change, `clipmem` stores:

- the whole clipboard snapshot
- each pasteboard item inside that snapshot
- every representation type exposed by that item
- raw bytes for every representation
- decoded text when the representation is text-like
- a searchable text projection for FTS5
- the frontmost application at capture time as a best-effort source hint

The database schema separates deduplicated `snapshots` from per-observation `capture_events`, so the same content copied ten times does not create ten full blob copies.

## Current behaviour

- Text, HTML, URLs, file URLs, RTF, JSON and XML are indexed when a reasonable text form is available.
- Images, PDFs and opaque binary types are fully stored as blobs but are not OCR’d.
- Search is lexical, backed by SQLite FTS5.
- The watcher polls `NSPasteboard.changeCount` on a short interval.
- The frontmost app is recorded as a practical hint, not a guaranteed pasteboard owner.

## Project layout

- `src/` – Rust source
- `extras/launchd/` – LaunchAgent template
- `extras/openclaw/clipboard-memory/` – OpenClaw skill stub
- `scripts/install_launchagent.sh` – install and load the watcher as a user LaunchAgent
- `scripts/uninstall_launchagent.sh` – remove the LaunchAgent
- `scripts/install_openclaw_skill.sh` – copy the skill into `~/.openclaw/skills`

## Build

You need a normal Rust toolchain on your Mac.

```bash
cargo build --release
```

Or install the CLI into `~/.local/bin`:

```bash
cargo install --path . --root ~/.local
```

That gives you:

```bash
~/.local/bin/clipmem
```

## Quick start

Capture the current clipboard once:

```bash
clipmem capture-once
```

Start the watcher in the foreground:

```bash
clipmem watch --interval-ms 350
```

Search the archive:

```bash
clipmem search "launchctl bootstrap" --limit 5
clipmem search "that shell one-liner with rsync" --json
```

Show recent unique clipboard states from the last 24 hours:

```bash
clipmem recent --hours 24
```

Inspect one stored snapshot:

```bash
clipmem get 42
clipmem get 42 --json
```

Check SQLite / FTS5 diagnostics:

```bash
clipmem doctor
```

## LaunchAgent install

The easiest route is:

```bash
./scripts/install_launchagent.sh
```

By default that will:

- install the Rust binary into `~/.local/bin`
- create `~/Library/Application Support/clipmem`
- write `~/Library/LaunchAgents/io.openclaw.clipmem.watch.plist`
- load and kickstart the user LaunchAgent

Useful environment variables for the script:

- `CLIPMEM_INSTALL_ROOT` – defaults to `~/.local`
- `CLIPMEM_DB_PATH` – defaults to `~/Library/Application Support/clipmem/clipmem.sqlite3`
- `CLIPMEM_INTERVAL_MS` – defaults to `350`

To remove it:

```bash
./scripts/uninstall_launchagent.sh
```

## OpenClaw

Install the bundled skill stub:

```bash
./scripts/install_openclaw_skill.sh
```

That copies:

```text
extras/openclaw/clipboard-memory/SKILL.md
```

into:

```text
~/.openclaw/skills/clipboard-memory
```

The skill tells OpenClaw to use:

- `clipmem search "<query>" --json`
- `clipmem recent --hours 24 --json`
- `clipmem get <snapshot-id> --json`

## Schema notes

The key tables are:

- `snapshots` – deduplicated clipboard states
- `snapshot_items` – items inside a snapshot
- `item_representations` – one row per item/type pair with raw blob storage
- `capture_events` – each time a snapshot was observed
- `snapshots_fts` – FTS5 external-content index over `snapshots.search_text`

## Limitations worth knowing

- Binary payloads are stored exactly, but only text-like payloads are indexed.
- RTF and HTML text extraction is intentionally lightweight.
- Search is great for commands, code, URLs, notes, logs and copied prose. It is not semantic search.
- This project is written to be easy to extend: adding export commands, embeddings, OCR, source-app heuristics or richer HTML parsing is straightforward.

## Example OpenClaw prompts once installed

- “Find that ffmpeg command I copied yesterday.”
- “Search my clipboard history for the SQL migration with WAL mode.”
- “What was the URL I copied from Safari about objc2 NSPasteboard?”
- “Show me the full clipboard entry for snapshot 128.”

## Development notes

The code is split so the database, search and tests compile cross-platform, while the actual capture implementation is behind `cfg(target_os = "macos")`.

There are a couple of unit tests for the database layer and text extraction helpers. On a Mac with Rust installed, run:

```bash
cargo test
```
