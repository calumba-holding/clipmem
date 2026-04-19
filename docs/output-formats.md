# Output formats

clipmem supports five output formats. Every retrieval command defaults
to a human-readable format and can switch to structured output for
scripts and agents.

## Choosing the right format

- **`text`** — human-readable terminal output. Don't parse this.
- **`json`** — single structured object with a stable envelope. Parse
  this for scripts and agents.
- **`jsonl`** — newline-delimited JSON rows. Use when streaming many
  results through a pipe.
- **`md`** — compact markdown. Human-oriented, default for `recall`.
- **`toon`** — flat, compact skim view with scalar columns only. Use
  for quick scans when you don't need nested fields.

## Format support by command

| Command | Default format | Supported formats |
|---|---|---|
| `recall` | `md` | `md`, `json`, `toon` |
| `search` | `text` | `text`, `json`, `jsonl`, `md`, `toon` |
| `recent` | `text` | `text`, `json`, `jsonl`, `md`, `toon` |
| `timeline` | `text` | `text`, `json`, `jsonl`, `md`, `toon` |
| `get` | `text` | `text`, `json`, `jsonl`, `md` |
| `restore` | `text` | `text`, `json` |
| `forget` | `text` | `text`, `json` |
| `purge` | `text` | `text`, `json` |
| `settings show` | `text` | `text`, `json` |
| `settings ignore list` | `text` | `text`, `json` |
| `service status` | `text` | `text`, `json` (via `--json`) |
| `capture-once` | — | `json` (via `--json`) |
| `doctor` | `text` | `text`, `json` (via `--json`) |
| `export` | raw bytes | `json` (write report) |

`--json` is a compatibility alias for `--format json` on `search`,
`recent`, `timeline`, `get`, `restore`, `forget`, `purge`, `export`,
`capture-once`, and `doctor`.

## JSON envelope

`search`, `recent`, `timeline`, and `recall` return a stable top-level
envelope:

```json
{
  "schema_version": 1,
  "command": "recall",
  "generated_at": "2026-04-17T12:34:56Z",
  "applied_filters": { "hours": 24, "app": "safari" },
  "truncated": false,
  "next_cursor": null,
  "results": []
}
```

- `schema_version` — integer. Breaking changes bump this value.
  Additive changes (new optional keys) are allowed within the same
  version.
- `command` — echoes the subcommand name.
- `generated_at` — RFC 3339 timestamp when the response was produced.
- `applied_filters` — echoes the filters actually applied.
- `truncated` — `true` when more rows exist beyond `--limit`.
- `next_cursor` — opaque string to pass back as `--cursor` when
  `truncated` is `true`. `null` when there are no more rows.
- `results` — array of flattened snapshot rows.

`recall` adds extra top-level fields:

- `best_candidate` — the top-ranked row (also `results[0]`)
- `why_selected` — short explanation of why this candidate was picked
- `best_match_confidence` — `"high"`, `"medium"`, or `"low"`
- `best_match_score` — float in `[0.0, 1.0]`
- `quoted_text` — present only when `--quote` is set and usable text
  exists

## Flattened text fields

`get --format json` and retrieval rows surface flat text fields so
you don't have to walk the nested representation tree:

- `best_text`, `best_text_uti` — the best available text and its UTI
- `text_fragments` — array of `{ representation, text }` objects
- `urls` — extracted URLs
- `file_paths` — extracted file paths
- `html_text`, `rtf_text` — extracted HTML and RTF text (if present)
- `text_summary` — concise summary for agents

`search` and `recall` additionally include:

- `why_matched` — short explanation of the match
- `matched_fields` — which indexed fields matched

The full nested `items[].representations[]` structure is still
available on `get` for deep inspection and export workflows.

## TOON skim output

`--format toon` is a compact skim view designed for quick scanning.
It keeps scalar columns that are easy for agents and humans to read;
it intentionally omits heavyweight fields.

- `search` / `recent` rows keep scalar identifiers, timestamps, app
  info, `display_text`, counts, bytes, score, and `why_matched`.
- `timeline` rows keep scalar event metadata plus `display_text`.
- `recall` keeps scalar best-match metadata and candidate rows.

Use `--format json` when you need `urls`, `file_paths`,
`text_fragments`, `best_text_uti`, `html_text`, `rtf_text`,
`matched_fields`, or any nested representation detail.

`--format toon` isn't supported on `get` because `get` returns nested
snapshot detail rather than flat list output.

## Action JSON output

Action commands return structured output when you request JSON:

- `restore --format json` — whether the snapshot was restored, the
  restored item count, representation count, and snapshot ID
- `forget --format json` — the deleted snapshot ID, event count, item
  count, representation count, and byte count
- `purge --format json` — age cutoff, dry-run state, and
  deleted-or-would-delete counts
- `export --format json` — destination path, byte count, UTI, item
  index, snapshot ID, and representation hash

## Script-friendly guarantees

clipmem is designed for script and agent consumption:

- stdout contains only the requested command output
- stderr contains diagnostics only
- No interactive prompts anywhere in the CLI
- List commands use bounded defaults and opaque cursor pagination
- `--format json` output is stable within `schema_version: 1`

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Uncategorized runtime failure |
| `2` | Invalid arguments |
| `3` | Not found (snapshot ID, representation, or no results) |
| `4` | Unsupported format for this subcommand |
| `5` | Database error |
| `6` | Platform error (macOS API or filesystem) |

Scripts can use these to distinguish "no such snapshot" (try a
different ID) from "database locked" (retry with backoff) from
"wrong format" (fix the `--format` flag).

## Next steps

- [Command reference](command-reference.md) — exhaustive flag-level
  reference for every command
- [Agent integration](agent-integration.md) — use clipmem with
  OpenClaw and other agent runtimes
