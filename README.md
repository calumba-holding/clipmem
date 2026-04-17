# clipmem

[![Crates.io](https://img.shields.io/crates/v/clipmem.svg)](https://crates.io/crates/clipmem)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust 1.87+](https://img.shields.io/badge/rust-1.87%2B-orange.svg)](https://www.rust-lang.org)

<p align="center">
  <img src="docs/clipmem-screenshot.png" alt="clipmem screenshot" width="600">
</p>

A searchable, local-only clipboard history for macOS, with a JSON-first CLI designed so agents — OpenClaw and others — can recall things you've copied.

`clipmem` watches the macOS system clipboard (`NSPasteboard`), archives every observed state into a local SQLite database, and exposes a handful of retrieval commands (`recall`, `recent`, `timeline`, `search`, `get`, `export`) that return either human-readable text or structured JSON.

## Contents

- [Requirements](#requirements)
- [Install](#install)
- [Quick start](#quick-start)
- [Run in the background (LaunchAgent)](#run-in-the-background-launchagent)
- [Recalling and searching](#recalling-and-searching)
- [Output formats](#output-formats)
- [Using with OpenClaw](#using-with-openclaw)
- [Privacy](#privacy)
- [Uninstall / Full cleanup](#uninstall--full-cleanup)
- [Limitations](#limitations)
- [How it works](#how-it-works)
- [Development](#development)
- [License](#license)

## Requirements

- **macOS.** Clipboard capture uses `NSPasteboard` via `objc2` / `objc2-app-kit` and is gated behind `cfg(target_os = "macos")`. The database and search layers compile on other Unixes for development, but capture will not.
- **Apple Silicon** for the prebuilt Homebrew package (target `aarch64-apple-darwin`). Intel Macs are supported via `cargo install`.
- **Rust 1.87+** for building from source (`rust-version` in `Cargo.toml`).

## Install

Homebrew (Apple Silicon):

```bash
brew install tristanmanchester/tap/clipmem
clipmem setup
# or: brew services start clipmem
```

Cargo (Apple Silicon or Intel):

```bash
cargo install clipmem
clipmem setup
```

Build from source:

```bash
cargo build --release
# or install the current checkout into ~/.local/bin
cargo install --path . --root ~/.local --force --locked
```

All three paths produce the same `clipmem` binary.

## Quick start

Initialize the archive and start background capture:

```bash
clipmem setup
```

Check service state or run the watcher in the foreground if you want to debug interactively:

```bash
clipmem service status
clipmem watch --interval-ms 350
```

Recall what you copied, or list recent items:

```bash
clipmem recall "what was that shell command?"
clipmem recent --hours 24
```

Check SQLite / FTS5 diagnostics if something looks off:

```bash
clipmem doctor
```

## Run in the background

`clipmem setup` is the canonical onboarding path. It performs one foreground capture to seed the archive, then starts background capture using the most natural provider for the active install:

- Homebrew installs prefer `brew services start clipmem`.
- Cargo and manual installs use a direct per-user LaunchAgent (`io.openclaw.clipmem.watch`).

Common service commands:

```bash
clipmem setup
clipmem service status
clipmem service stop
clipmem service uninstall
```

If you prefer the Homebrew-native flow, this is also supported:

```bash
brew services start clipmem
brew services stop clipmem
```

The compatibility wrappers `./scripts/install_launchagent.sh` and `./scripts/uninstall_launchagent.sh` still exist, but they now delegate to `clipmem setup` and `clipmem service uninstall`.

For a full wipe including the database, see [Uninstall / Full cleanup](#uninstall--full-cleanup).

## Recalling and searching

Start with the command that returns the least structure needed for the job:

- `clipmem recall` — best-first answer plus a few alternatives. The primary agent-facing retrieval command.
- `clipmem recent` — recent unique clipboard states, deduplicated by snapshot.
- `clipmem timeline` — chronological capture events, including repeated copies of the same content.
- `clipmem search` — direct lexical matching over stored text.
- `clipmem get <snapshot-id>` — nested item/representation detail for a single snapshot.
- `clipmem export <snapshot-id> --item <n> --uti <uti> --out <path>` — raw bytes for binary payloads.

### Common recipes

```bash
# Best-first answer
clipmem recall "what was that git one-liner?"
clipmem recall --prefer-recent --hours 24 --format json
clipmem recall "Terminal stuff" --prefer-app terminal --format toon

# Recent unique items from one app
clipmem recent --hours 24 --app safari --format md
clipmem recent --hours 24 --format toon

# Paginated chronological history
clipmem timeline --hours 24 --limit 25 --format json
clipmem timeline --hours 24 --limit 25 --cursor "<next_cursor>" --format json

# Lexical search, forcing a mode
clipmem search "launchctl bootstrap"
clipmem search --mode literal "50%"
clipmem search --mode fts "\"launchctl\" AND bootstrap"

# Detail + raw bytes
clipmem get 42 --format json
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png
```

### Shared retrieval filters

`search`, `recent`, `timeline`, and `recall` all accept the same filters. `get` and `export` accept them as guards against the explicitly targeted snapshot.

**Time:**

- `--since <RFC3339>` — captures at or after this timestamp (e.g. `2026-04-16T09:00:00Z`).
- `--until <RFC3339>` — captures at or before this timestamp.
- `--hours <N>` — last N hours, unless `--since` is also provided (then `--since` wins).

**Source:**

- `--app <name>` — case-insensitive substring match on the recorded frontmost app name.
- `--bundle-id <id>` — case-insensitive exact match on bundle identifier, e.g. `com.apple.Safari`.

**Content shape:**

- `--kind text|html|rtf|url|file|image|pdf|binary|other` (`file` means file URLs; `other` means mixed or empty snapshots).
- `--has-text`, `--has-url`, `--has-file-url`, `--has-image`, `--has-pdf` — presence flags are additive with AND semantics.

**Size:**

- `--min-bytes <N>` / `--max-bytes <N>`.

### Pagination

Every list command accepts `--limit` (bounded 1–250, default 10) plus an opaque `--cursor` returned as `next_cursor` in a prior response:

```bash
clipmem search "git status" --format json --limit 10
clipmem search "git status" --format json --limit 10 --cursor "<next_cursor>"
```

Cursors are opaque and tied to the active query, mode, and filters.

### Search modes

`search` and `recall` accept `--mode auto|fts|literal`. Auto mode picks FTS or literal per query — you rarely need to override. It prefers literal matching for URLs, paths, bundle ids, dotted identifiers, and shell-like fragments (more reliable for punctuation-heavy clipboard data); plain-text queries can still use FTS first.

### `recall` extras

On top of the shared retrieval filters, `recall` supports:

- `--format md|json|toon` (default: `md`)
- `--limit` — ranked candidates to consider (default 5)
- `--full` — expand the best candidate text instead of the compact form
- `--quote` — force quoted best-text output
- `--min-score <0.0-1.0>` — minimum normalized match score before a query stands on its own
- `--prefer-recent` — bias ranking toward recency
- `--prefer-app <name>` — bias toward matching app or bundle id
- `--hours <N>` — window for the recent fallback when a query is weak

## Output formats

Human by default, machine-readable on request:

```bash
# Human-readable terminal output (default for search/recent/timeline/get)
clipmem recent --hours 24

# Machine-readable JSON for scripts and agents
clipmem recent --hours 24 --format json
```

Supported formats, by command:

- `search`, `recent`, `timeline`, `get`: `text` (default), `json`, `jsonl`, `md`, `toon` (*not* `toon` for `get`, which returns nested detail).
- `recall`: `md` (default), `json`, `toon`.
- `capture-once` and `doctor`: `--json` only (alias for structured output).
- `export`: writes raw bytes to `--out`; no `--format`.

`--json` on `search`, `recent`, `timeline`, `get`, `capture-once`, and `doctor` is a compatibility alias for `--format json`.

`--format toon` is a compact skim view, not a near-lossless mirror of the JSON payload. TOON keeps scalar columns that are easy for agents and humans to scan quickly; if you need URLs, file paths, text fragments, representation UTIs, or other richer fields, use `--format json` or `clipmem get`.

### JSON envelope

`search`, `recent`, `timeline`, and `recall` return a stable top-level envelope:

- `schema_version`
- `command`
- `generated_at`
- `applied_filters`
- `truncated`
- `next_cursor`
- `results`

### Flattened text fields

`get --format json` and retrieval rows surface flat text fields so agents don't have to walk the nested representation tree:

- `best_text`, `best_text_uti`
- `text_fragments`
- `urls`, `file_paths`
- `html_text`, `rtf_text`
- `text_summary`

`search` and `recall` additionally include:

- `why_matched`
- `matched_fields`

The full nested `items[].representations[]` structure is still present on `get` for deep inspection and export workflows.

### TOON skim output

TOON is intentionally narrower than JSON:

- `search` / `recent` rows keep scalar identifiers, timestamps, app info, `display_text`, counts, bytes, score, and `why_matched`.
- `timeline` rows keep scalar event metadata plus `display_text`.
- `recall` keeps scalar best-match metadata and candidate rows, but does not inline heavyweight fields as JSON strings.

Use `--format json` when you need `urls`, `file_paths`, `text_fragments`, `best_text_uti`, `html_text`, `rtf_text`, `matched_fields`, or any nested representation detail.

### Script-friendly by design

- stdout contains only the requested command output
- stderr contains diagnostics only
- no interactive prompts anywhere
- list commands use bounded defaults and opaque cursor pagination

Exit codes:

- `0` — success
- `2` — invalid args
- `3` — not found
- `4` — unsupported format
- `5` — database error
- `6` — platform error
- `1` — uncategorized runtime failure

## Using with OpenClaw

Install the packaged skill via the binary rather than by copying files:

```bash
clipmem agents openclaw install-skill
```

By default this installs into the active OpenClaw workspace:

```text
~/.openclaw/workspace/skills/clipboard-memory
```

The workspace root is resolved from `openclaw config get agents.defaults.workspace`, falling back to `~/.openclaw/workspace`.

To install into the shared OpenClaw skills directory instead:

```bash
clipmem agents openclaw install-skill --shared
# writes to ~/.openclaw/skills/clipboard-memory
```

Or to a specific destination:

```bash
clipmem agents openclaw install-skill --dest /path/to/skill --force
```

Other useful commands:

```bash
clipmem agents openclaw doctor         # check PATH, workspace, installed files, sandbox
clipmem agents openclaw print-skill    # print the packaged SKILL.md
clipmem agents openclaw uninstall-skill
```

### What the skill ships

```text
SKILL.md
references/commands.md
references/troubleshooting.md
```

The packaged skill points OpenClaw at the JSON-first retrieval commands:

- `clipmem recall "<query>" --format json`
- `clipmem recall --prefer-recent --hours 24 --format json`
- `clipmem timeline ... --format json` when chronology matters
- `clipmem search ... --format json` when direct lexical matching matters
- `clipmem get <snapshot-id> --format json` when deeper nested detail is needed

### Troubleshooting

OpenClaw must be able to run `clipmem` from its own environment, not just from your interactive shell. If you installed `clipmem` into `~/.local/bin`, make sure that directory is on the PATH seen by OpenClaw. If sandboxing is active, the binary may also need to be available inside the sandbox image or container.

Start with:

```bash
clipmem agents openclaw doctor
openclaw sandbox explain   # when available
```

For empty results, sandbox visibility, or binary-only (image / PDF) snapshots where exact text isn't available, see [`extras/openclaw/clipboard-memory/references/troubleshooting.md`](extras/openclaw/clipboard-memory/references/troubleshooting.md).

### Example OpenClaw prompts once installed

- "Find that ffmpeg command I copied yesterday."
- "Search my clipboard history for the SQL migration with WAL mode."
- "What was the URL I copied from Safari about objc2 `NSPasteboard`?"
- "Show me the full clipboard entry for snapshot 128."

### Skill packaging (reference)

- `skills/clipboard-memory/` — the canonical cross-agent skill source that `skills.sh` discovers from the repo root.
- `extras/openclaw/clipboard-memory/` — the OpenClaw-native package installed by `clipmem agents openclaw install-skill`.
- `extras/agent-skills/clipboard-memory/` — a portable packaging mirror for generic agent-skill runtimes.

For compatibility, `./scripts/install_openclaw_skill.sh` still exists as a thin wrapper around `clipmem agents openclaw install-skill --shared --force`.

## Privacy

- All clipboard data stays on your machine. `clipmem` makes no network calls and sends no telemetry.
- The database lives at `~/Library/Application Support/clipmem/clipmem.sqlite3`. Logs are in the same directory under `logs/`.
- Permissions: directory `0700`, database and logs `0600` — enforced at runtime in `src/db.rs::harden_path_permissions` and by the managed service setup flow.
- **The database is not encrypted.** Rely on FileVault (or similar disk encryption) for at-rest protection.
- There is **no automatic retention or pruning**. The archive grows until you delete it. See [Uninstall / Full cleanup](#uninstall--full-cleanup) to wipe.

## Uninstall / Full cleanup

```bash
# stop and remove the managed background service
clipmem service uninstall

# remove the OpenClaw skill (if installed)
clipmem agents openclaw uninstall-skill

# delete the database and logs
rm -rf ~/Library/Application\ Support/clipmem

# remove the binary
cargo uninstall clipmem || rm -f ~/.local/bin/clipmem

# if installed via Homebrew
brew uninstall clipmem 2>/dev/null || true
```

## Limitations

- Text, HTML, URLs, file URLs, RTF, JSON, and XML are indexed when a reasonable text form is available. Images, PDFs, and opaque binary types are stored as blobs but are not OCR'd.
- The watcher polls `NSPasteboard.changeCount` on a short interval. It is best-effort — if the clipboard changes multiple times within the poll window (400ms by default), intermediate states can be missed.
- The frontmost app is recorded as a practical hint, not a guaranteed pasteboard owner.
- RTF and HTML text extraction is intentionally lightweight.
- Search is great for commands, code, URLs, notes, logs, and copied prose. It is not semantic search. `--mode auto` is the default; use `--mode fts` for strict FTS5 queries or `--mode literal` for exact substring matching.
- `clipmem get --format json` omits raw blob bytes (flattened text fields are included). Use `clipmem export` to recover binary payloads.
- `--format toon` is only supported for skim output (`search`, `recent`, `timeline`, `recall`), not for nested `get` detail. It intentionally omits heavyweight fields instead of embedding them as JSON blobs.

## How it works

For each observed clipboard state, `clipmem` stores:

- the whole clipboard snapshot
- each pasteboard item inside that snapshot
- every representation type the item exposes (identified by UTI — Uniform Type Identifier, e.g. `public.png`, `public.url`)
- raw bytes for every representation
- decoded text when the representation is text-like, plus strict UTF-8/UTF-16 recovery for some byte-only payloads
- a searchable text projection that powers default retrieval
- the frontmost application at capture time as a best-effort source hint

Identical clipboard snapshots are deduplicated by SHA-256 fingerprint, so copying the same thing ten times does not create ten full blob copies — it creates one snapshot and ten `capture_events`.

Default search uses SQLite's built-in full-text search (FTS5) when that fits the query, and falls back to literal substring matching for wildcard-like input, invalid FTS syntax, or zero FTS hits.

### Schema

- `snapshots` — deduplicated clipboard states
- `snapshot_items` — items inside a snapshot
- `item_representations` — one row per item/type pair, with raw blob storage
- `capture_events` — each time a snapshot was observed
- `snapshots_fts` — FTS5 external-content index over `snapshots.search_text`

## Development

Project layout:

- `src/` — Rust source. Capture is gated behind `cfg(target_os = "macos")`; database, search, and tests compile cross-platform.
- `extras/launchd/` — LaunchAgent plist template.
- `skills/clipboard-memory/` — canonical public skill content for repo-based installers.
- `extras/openclaw/clipboard-memory/` — packaged OpenClaw-native skill content.
- `extras/agent-skills/clipboard-memory/` — portable skill package mirror for non-OpenClaw runtimes.
- `scripts/install_launchagent.sh` / `scripts/uninstall_launchagent.sh` — compatibility wrappers around `clipmem setup` and `clipmem service uninstall`.
- `scripts/install_openclaw_skill.sh` — compatibility wrapper around `clipmem agents openclaw install-skill`.

Build and test (Rust 1.87+):

```bash
cargo build --release
cargo test
```

The code is written to make extension straightforward — export commands, embeddings, OCR, richer source-app heuristics, or fuller HTML parsing all fit the existing shape.

See [`RELEASING.md`](RELEASING.md) for the release workflow.

## License

`clipmem` is released under the MIT License. See [LICENSE](LICENSE).
