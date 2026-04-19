# Command reference

Exhaustive flag-level reference for every `clipmem` command and
subcommand. For conceptual explanations and usage guidance, see
[searching and filtering](searching-and-filtering.md),
[output formats](output-formats.md), and
[managing your archive](managing-your-archive.md).

## Subcommand overview

| Command | Default format | Supports `toon`? | Purpose |
|---|---|---|---|
| `recall [QUERY]` | `md` | yes | Ranked best-first answer with alternatives |
| `search <QUERY>` | `text` | yes | Lexical / FTS match over the archive |
| `recent` | `text` | yes | Recent unique snapshots (deduplicated) |
| `timeline` | `text` | yes | Chronological capture events (not deduplicated) |
| `stats` | `text` | no | Archive aggregates and leaderboards |
| `get <ID>` | `text` | no | Nested detail for one snapshot |
| `restore <ID>` | `text` | — | Restore a stored snapshot to the clipboard |
| `export <ID>` | raw bytes | — | Write one representation to disk |
| `forget <ID>` | `text` | — | Hard-delete one snapshot and its history |
| `purge` | `text` | — | Delete old snapshots by age |
| `settings show` | `text` | no | Show capture policy |
| `settings pause` | `text` | — | Pause or resume capture |
| `settings api-key-filter` | `text` | — | Enable or disable API key filtering |
| `settings ocr` | `text` | — | Enable or disable local OCR for new captures |
| `settings retention` | `text` | — | Set retention duration |
| `settings ignore` | `text` | no | Manage ignored bundle IDs |
| `ocr status` | `text` | — | Show OCR queue and result counts |
| `ocr run` | `text` | — | Backfill OCR for stored image snapshots |
| `setup` | — | — | Initialize and start background capture |
| `service start` | — | — | Start background capture |
| `service stop` | — | — | Stop background capture |
| `service status` | `text` | — | Report watcher state and freshness |
| `service uninstall` | — | — | Remove managed service definition |
| `watch` | — | — | Continuous foreground polling |
| `capture-once` | — | — | Single clipboard capture |
| `doctor` | `text` | — | SQLite and FTS5 diagnostics |
| `agents openclaw install-skill` | — | — | Install packaged skill |
| `agents openclaw uninstall-skill` | — | — | Remove installed skill |
| `agents openclaw print-skill` | — | — | Print SKILL.md to stdout |
| `agents openclaw doctor` | `text` | — | Integration health check |

## Global flags

- `--db <path>` — override the SQLite database path. Default:
  `~/Library/Application Support/clipmem/clipmem.sqlite3`.

## Shared retrieval filters

These flags are accepted by `search`, `recent`, `timeline`, `stats`,
`recall`, `get`, and `export`:

| Flag | Type | Description |
|---|---|---|
| `--since <RFC3339>` | timestamp | Captures at or after this time |
| `--until <RFC3339>` | timestamp | Captures at or before this time |
| `--hours <N>` | integer | Last N hours (ignored if `--since` set) |
| `--app <name>` | string | Case-insensitive substring match on app name |
| `--bundle-id <id>` | string | Case-insensitive exact match on bundle ID |
| `--kind <type>` | enum | `text`, `html`, `rtf`, `url`, `file`, `image`, `pdf`, `binary`, `other` |
| `--has-text` | flag | Require non-empty text representation |
| `--has-url` | flag | Require URL representation |
| `--has-file-url` | flag | Require file URL representation |
| `--has-image` | flag | Require image representation |
| `--has-pdf` | flag | Require PDF representation |
| `--min-bytes <N>` | integer | Minimum snapshot byte count |
| `--max-bytes <N>` | integer | Maximum snapshot byte count |

---

## Retrieval commands

### `clipmem recall [QUERY]`

Best-first ranked answer with alternatives.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `md\|json\|toon` | `md` | Output format |
| `--mode` | `auto\|fts\|literal` | `auto` | Search mode |
| `--limit` | 1-250 | 5 | Ranked candidates to consider |
| `--full` | flag | — | Expand best candidate text |
| `--quote` | flag | — | Force quoted best-text output |
| `--min-score` | 0.0-1.0 | — | Minimum match score threshold |
| `--prefer-recent` | flag | — | Bias ranking toward recency |
| `--prefer-app` | string | — | Bias toward matching app |
| + shared filters | | | |

```bash
clipmem recall "what was that command I copied?"
clipmem recall "find the URL" --format json
clipmem recall --prefer-recent --hours 24
clipmem recall "exact text" --quote --full
```

### `clipmem search <QUERY>`

Direct lexical matching over stored text.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json\|jsonl\|md\|toon` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |
| `--mode` | `auto\|fts\|literal` | `auto` | Search mode |
| `--limit` | 1-250 | 10 | Page size |
| `--cursor` | string | — | Resume from prior `next_cursor` |
| + shared filters | | | |

```bash
clipmem search "git commit -m"
clipmem search "https://example.com" --format json
clipmem search --mode literal "foo:bar"
clipmem search --mode fts "\"launchctl\" AND bootstrap"
```

### `clipmem recent`

Recent unique clipboard states, deduplicated by snapshot.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json\|jsonl\|md\|toon` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |
| `--limit` | 1-250 | 10 | Page size |
| `--cursor` | string | — | Resume from prior `next_cursor` |
| + shared filters | | | |

```bash
clipmem recent
clipmem recent --hours 24 --app safari
clipmem recent --format json --limit 25 --cursor "<next_cursor>"
```

### `clipmem timeline`

Chronological capture events, one row per observation.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json\|jsonl\|md\|toon` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |
| `--sort` | `asc\|desc` | `desc` | Chronological sort order |
| `--limit` | 1-250 | 10 | Page size |
| `--cursor` | string | — | Resume from prior `next_cursor` |
| + shared filters | | | |

```bash
clipmem timeline --hours 24
clipmem timeline --sort asc --format json
clipmem timeline --app safari --has-url --limit 25 --format json
```

### `clipmem stats`

Archive aggregates and leaderboards for matching captures and
snapshots.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |
| + shared filters | | | |

```bash
clipmem stats
clipmem stats --hours 24
clipmem stats --app safari --format json
```

### `clipmem get <SNAPSHOT_ID>`

Nested item/representation detail for a single snapshot.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json\|jsonl\|md` | `text` | Output format (no `toon`) |
| `--json` | flag | — | Alias for `--format json` |
| `--events` | 1-250 | 10 | Recent capture events to include |
| + shared filters | | | |

```bash
clipmem get 42
clipmem get 42 --format json
clipmem get 42 --events 25 --format md
```

---

## Action commands

### `clipmem restore <SNAPSHOT_ID>`

Restore the full stored snapshot back onto the macOS clipboard.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

```bash
clipmem restore 42
clipmem restore 42 --format json
```

### `clipmem export <SNAPSHOT_ID>`

Write one stored representation as raw bytes to a file.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--item` | integer | required | 0-based item index |
| `--uti` | string | required | Representation UTI |
| `--out` | path | required | Destination file path |
| `--force` | flag | — | Replace existing regular file |
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |
| + shared filters | | | |

```bash
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png \
  --force
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png \
  --format json
```

### `clipmem forget <SNAPSHOT_ID>`

Irreversibly delete one snapshot and its capture history.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

```bash
clipmem forget 42
clipmem forget 42 --format json
```

### `clipmem purge`

Delete snapshots older than a given duration.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--older-than` | duration | required | Age threshold (`Nd`, `Nh`, `Nm`) |
| `--dry-run` | flag | — | Preview without deleting |
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

```bash
clipmem purge --older-than 30d
clipmem purge --older-than 12h --dry-run
clipmem purge --older-than 30d --dry-run --format json
```

---

## Management commands

### `clipmem setup`

Initialize the database, seed one capture, and start background
capture.

```bash
clipmem setup
clipmem setup --db /tmp/clipmem.sqlite3
```

Re-running `setup` is safe and updates the managed service definition.

### `clipmem service start`

Start background capture using the preferred service provider.

### `clipmem service stop`

Stop background capture without uninstalling the service definition.

### `clipmem service status`

Report provider state, freshness, and service wiring.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--json` | flag | — | Emit status as JSON |

```bash
clipmem service status
clipmem service status --json
```

### `clipmem service uninstall`

Remove the managed service definition.

### `clipmem watch`

Continuously poll the clipboard and archive observed changes.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--interval-ms` | integer | 400 | Poll interval in milliseconds |
| `--quiet` | flag | — | Suppress per-capture status lines |
| `--skip-initial` | flag | — | Skip the already-present clipboard state |

```bash
clipmem watch
clipmem watch --interval-ms 350
clipmem watch --skip-initial --quiet
```

Intervals below 50ms are clamped to 50ms at runtime.

### `clipmem capture-once`

Capture the current clipboard state once.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--json` | flag | — | Emit captured snapshot as JSON |

```bash
clipmem capture-once
clipmem capture-once --json
```

### `clipmem ocr status`

Show OCR queue counts and how many snapshots have ready OCR text.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

```bash
clipmem ocr status
clipmem ocr status --format json
```

### `clipmem ocr run`

Process pending OCR work for existing image snapshots. This is the
backfill path for images copied before OCR was enabled.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | 1-250 | 25 | Maximum image hashes to process |
| `--snapshot` | integer | — | Restrict processing to one snapshot ID |
| `--retry-failed` | flag | — | Requeue failed OCR hashes before running |
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

```bash
clipmem ocr run
clipmem ocr run --limit 50
clipmem ocr run --snapshot 42 --retry-failed --format json
```

---

## Settings commands

### `clipmem settings show`

Show the current capture policy.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

### `clipmem settings pause <on|off>`

Persistently pause (`on`) or resume (`off`) capture.

### `clipmem settings api-key-filter <on|off>`

Enable (`on`) or disable (`off`) API key filtering.

### `clipmem settings ocr <on|off>`

Enable (`on`) or disable (`off`) automatic local OCR for new image
captures. OCR is off by default. Turning it on doesn't rewrite stored
image bytes or compress images.

### `clipmem settings retention <duration|forever>`

Set retention to a duration (`30d`, `12h`, `15m`) or `forever` to
disable automatic pruning.

### `clipmem settings ignore add <BUNDLE_ID>`

Add a bundle ID to the ignore list.

### `clipmem settings ignore remove <BUNDLE_ID>`

Remove a bundle ID from the ignore list.

### `clipmem settings ignore list`

List ignored bundle IDs.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--format` | `text\|json` | `text` | Output format |
| `--json` | flag | — | Alias for `--format json` |

---

## Diagnostics

### `clipmem doctor`

Print SQLite and FTS5 diagnostics.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--json` | flag | — | Emit diagnostics as JSON |

```bash
clipmem doctor
clipmem doctor --json
```

A nonzero exit code means required checks failed.

---

## Agent integration

### `clipmem agents openclaw install-skill`

Install the packaged clipboard-memory skill into OpenClaw.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--shared` | flag | — | Install to shared directory (`~/.openclaw/skills/`) |
| `--dest` | path | — | Install to a specific directory |
| `--force` | flag | — | Replace an existing skill directory |

```bash
clipmem agents openclaw install-skill
clipmem agents openclaw install-skill --shared
clipmem agents openclaw install-skill --dest /path/to/skill --force
```

### `clipmem agents openclaw uninstall-skill`

Remove an installed skill directory.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--shared` | flag | — | Remove from shared directory |
| `--dest` | path | — | Remove from a specific directory |

### `clipmem agents openclaw print-skill`

Print the packaged SKILL.md to stdout for inspection.

### `clipmem agents openclaw doctor`

Check host PATH, installed skill state, and sandbox guidance.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--shared` | flag | — | Check shared directory |
| `--dest` | path | — | Check a specific directory |

```bash
clipmem agents openclaw doctor
clipmem agents openclaw doctor --shared
```

A nonzero exit code means required integration checks failed.

---

## Environment variables

- `CLIPMEM_OPENCLAW_WORKSPACE` — overrides the OpenClaw workspace root
  used by `agents openclaw install-skill` and `agents openclaw doctor`.
  Falls back to `openclaw config get agents.defaults.workspace`, then
  `~/.openclaw/workspace`.
- `HOME` — resolves `~/` in default paths.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Uncategorized runtime failure |
| `2` | Invalid arguments |
| `3` | Not found |
| `4` | Unsupported format for this subcommand |
| `5` | Database error |
| `6` | Platform error (macOS API or filesystem) |
