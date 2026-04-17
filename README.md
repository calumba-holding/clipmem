# clipmem

<p align="center">
  <img src="docs/clipmem-screenshot.png" alt="clipmem screenshot" width="600">
</p>

`clipmem` is a small Rust CLI for macOS that archives observed clipboard states into SQLite, builds a searchable local clipboard history, and gives you a command that OpenClaw can call whenever it needs to recall something you copied.

It captures the current `NSPasteboard` contents, stores every representation it can read for every pasteboard item, deduplicates identical clipboard snapshots by SHA-256 fingerprint, and records each observed state as its own event.

## What it stores

For each observed clipboard state, `clipmem` stores:

- the whole clipboard snapshot
- each pasteboard item inside that snapshot
- every representation type exposed by that item
- raw bytes for every representation
- decoded text when the representation is text-like, plus strict UTF-8/UTF-16 recovery for some byte-only payloads
- a searchable text projection for recognized text-like representations that powers automatic FTS-or-literal lookup
- the frontmost application at capture time as a best-effort source hint

The database schema separates deduplicated `snapshots` from per-observation `capture_events`, so the same content copied ten times does not create ten full blob copies.

The local archive is treated as sensitive state: the tool and installer tighten archive, log, and LaunchAgent file permissions to the current user on platforms that support POSIX modes.

## Current behaviour

- Text, HTML, URLs, file URLs, RTF, JSON and XML are indexed when a reasonable text form is available.
- Images, PDFs and opaque binary types are fully stored as blobs but are not OCR’d.
- Default search is automatic: it uses SQLite FTS5 when that fits the query and falls back to literal substring matching for wildcard-like input, invalid FTS syntax, or zero FTS hits.
- The watcher polls `NSPasteboard.changeCount` on a short interval.
- The watcher is best-effort: if the clipboard changes multiple times between polls, intermediate states can be missed.
- The frontmost app is recorded as a practical hint, not a guaranteed pasteboard owner.

## Project layout

- `src/` – Rust source
- `extras/launchd/` – LaunchAgent template
- `extras/openclaw/clipboard_memory/` – OpenClaw skill stub
- `scripts/install_launchagent.sh` – install and load the watcher as a user LaunchAgent
- `scripts/uninstall_launchagent.sh` – remove the LaunchAgent
- `scripts/install_openclaw_skill.sh` – copy the skill into `~/.openclaw/skills`

## Install

Published installs:

```bash
brew install tristanmanchester/tap/clipmem
```

Or, if you already have Rust installed:

```bash
cargo install clipmem
```

The Homebrew package is intended for Apple Silicon Macs. On Intel Macs, prefer:

```bash
cargo install clipmem
```

Build from source:

```bash
cargo build --release
```

Or install the current checkout into `~/.local/bin`:

```bash
cargo install --path . --root ~/.local --force --locked
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

Skip storing the clipboard state that already exists when a watcher starts:

```bash
clipmem watch --interval-ms 350 --skip-initial
```

Search the archive:

```bash
clipmem search "launchctl bootstrap" --limit 5
clipmem search "that shell one-liner with rsync" --json
clipmem search --mode literal "50%"
clipmem search --mode fts "\"launchctl\" AND bootstrap"
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

Export a stored raw representation:

```bash
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png
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
- configure the LaunchAgent to start with `--skip-initial`
- load and kickstart the user LaunchAgent

Useful environment variables for the script:

- `CLIPMEM_INSTALL_ROOT` – defaults to `~/.local`
- `CLIPMEM_DB_PATH` – defaults to `~/Library/Application Support/clipmem/clipmem.sqlite3`
- `CLIPMEM_INTERVAL_MS` – defaults to `350`

To remove the LaunchAgent and plist:

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
extras/openclaw/clipboard_memory/SKILL.md
```

into:

```text
~/.openclaw/skills/clipboard_memory
```

The skill tells OpenClaw to use:

- `clipmem search "<query>" --json`
- `clipmem recent --hours 24 --json`
- `clipmem get <snapshot-id> --json`

OpenClaw must be able to run `clipmem` from its own environment, not just from your interactive shell. If you installed `clipmem` into `~/.local/bin`, make sure that directory is on the PATH seen by OpenClaw.

After copying the skill, reload skills or restart OpenClaw if it does not appear immediately.

## Schema notes

The key tables are:

- `snapshots` – deduplicated clipboard states
- `snapshot_items` – items inside a snapshot
- `item_representations` – one row per item/type pair with raw blob storage
- `capture_events` – each time a snapshot was observed
- `snapshots_fts` – FTS5 external-content index over `snapshots.search_text`

## Limitations worth knowing

- Binary payloads are stored exactly, but only recognized text-like payloads are indexed.
- `clipmem get --json` intentionally omits raw blob bytes. Use `clipmem export` to recover stored binary payloads.
- RTF and HTML text extraction is intentionally lightweight and best-effort.
- Search is great for commands, code, URLs, notes, logs and copied prose. Use `--mode auto` for the default FTS-or-literal behavior, `--mode fts` for strict FTS5 queries, or `--mode literal` for exact substring matching. It is not semantic search.
- Explicit `--mode fts` keeps SQLite FTS5 semantics. In automatic mode, punctuation-heavy inputs may fall back to literal search.
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

## Release automation

Releases are tag-driven. Pushing a semver tag like `v0.1.0` triggers:

- validation that the git tag matches `Cargo.toml`
- test and package verification
- crates.io publish
- GitHub Release asset generation via `cargo-dist`
- Homebrew tap updates for `tristanmanchester/homebrew-tap`

Repo-side release files:

- `.github/workflows/ci.yml` – push and pull request validation
- `.github/workflows/release.yml` – generated by `cargo-dist` for tag-driven releases
- `.github/workflows/publish-crate.yml` – reusable bootstrap publish job for crates.io
- `dist-workspace.toml` – `cargo-dist` release configuration

Bootstrap for `0.1.0`:

1. Create the tap repo `tristanmanchester/homebrew-tap`.
2. Add the GitHub Actions secret `HOMEBREW_TAP_TOKEN` with write access to that repo.
3. Add a temporary `CARGO_REGISTRY_TOKEN` secret for the first crates.io publish.
4. Ensure the crates.io account behind that token has a verified email address.
5. Push tag `v0.1.0`.

After `0.1.0` is live on crates.io, migrate the crates publish step from the temporary `CARGO_REGISTRY_TOKEN` secret to crates.io Trusted Publishing with `rust-lang/crates-io-auth-action@v1`, then remove the long-lived secret.
