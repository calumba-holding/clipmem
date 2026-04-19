# Searching and filtering

clipmem provides several retrieval commands, each optimized for a
different query pattern. All of them share the same filter set and
pagination model.

## Choosing the right command

Start with the command that returns the least structure needed for
the job:

- **Have a vague memory?** Start with `clipmem recall`. It returns a
  best-first ranked answer with alternatives.
- **Want chronological order?** Use `clipmem timeline`. It shows one
  row per capture event, including repeated copies of the same content.
- **Want archive aggregates?** Use `clipmem stats`. It summarizes
  matching events, distinct snapshots, app activity, content mix, and
  leaderboards.
- **Want recent unique items?** Use `clipmem recent`. It deduplicates
  by snapshot, showing each unique clipboard state once.
- **Need exact substring matching?** Use `clipmem search`. It does
  direct lexical matching over stored text.
- **Already have a snapshot ID?** Use `clipmem get` for full nested
  detail of a single snapshot.

## Recall

`recall` is the primary retrieval command. It returns a ranked best
answer with alternatives, making it ideal for questions like "what
was that thing I copied?"

```bash
clipmem recall "what was that git one-liner?"
clipmem recall --prefer-recent --hours 24 --format json
clipmem recall "Terminal stuff" --prefer-app terminal --format toon
```

### Recall-specific flags

On top of the shared retrieval filters, `recall` supports:

- `--format md|json|toon` — output format (default: `md`)
- `--limit` — ranked candidates to consider (default: 5)
- `--full` — expand the best candidate text instead of the compact form
- `--quote` — force quoted best-text output
- `--min-score <0.0-1.0>` — minimum normalized match score before a
  query stands on its own
- `--prefer-recent` — bias ranking toward recency
- `--prefer-app <name>` — bias toward matching app or bundle ID
- `--hours <N>` — window for the recent fallback when a query is weak

When you don't have a query but the user said "the thing I just
copied":

```bash
clipmem recall --prefer-recent --hours 24 --format json --limit 5
```

## Recent

`recent` returns unique clipboard states, deduplicated by snapshot.
Use it for "show me the recent things I copied" queries.

```bash
clipmem recent --hours 24 --app safari --format md
clipmem recent --hours 24 --format toon
```

## Timeline

`timeline` returns chronological capture events — one row per real
copy. Repeated copies of the same content appear as separate events.

```bash
clipmem timeline --hours 24
clipmem timeline --hours 24 --sort asc --format json
clipmem timeline --app safari --has-url --limit 25 --format json
```

Use `--sort asc|desc` to control chronological order (default: `desc`).

## Stats

`stats` returns aggregate archive metrics for the active filters. It
counts matching capture events separately from distinct matching
snapshots, so repeated copies affect event totals and dedupe ratio
without inflating unique snapshot counts.

```bash
clipmem stats
clipmem stats --hours 24
clipmem stats --app safari --format json
```

## Search

`search` does direct lexical matching over stored text. It's best for
exact phrases, URLs, commands, and punctuation-heavy strings.

```bash
clipmem search "launchctl bootstrap"
clipmem search --mode literal "50%"
clipmem search --mode fts "\"launchctl\" AND bootstrap"
```

## OCR text

When OCR is enabled or backfilled, completed OCR text participates in
default `search` and `recall` results. OCR matches report
`matched_fields = ["ocr_text"]` in JSON output and include a
`why_matched` explanation that names OCR text.

Use these commands to control and inspect OCR:

```bash
clipmem settings ocr on
clipmem ocr status
clipmem ocr run --limit 25
```

OCR is opt-in for new captures. Use `clipmem ocr run` to backfill
existing image snapshots. OCR text is stored separately from native
snapshot text, so the original image bytes stay unchanged for
`restore` and `export`.

## Get

`get` returns the full nested detail for a single snapshot, including
all items, representations, and recent capture events.

```bash
clipmem get 42 --format json
clipmem get 42 --events 25 --format md
```

Use `--events <N>` to control how many recent capture events to
include (default: 10, bounded 1-250).

`get` doesn't support `--format toon` because it returns nested
snapshot detail rather than flat list output.

## Shared retrieval filters

`search`, `recent`, `timeline`, `stats`, and `recall` accept the same
filter set. `get` and `export` accept them as guards against the
explicitly targeted snapshot.

### Time

- `--since <RFC3339>` — captures at or after this timestamp (for
  example, `2026-04-16T09:00:00Z`)
- `--until <RFC3339>` — captures at or before this timestamp
- `--hours <N>` — last N hours, unless `--since` is also provided
  (then `--since` takes precedence)

### Source

- `--app <name>` — case-insensitive substring match on the recorded
  frontmost app name
- `--bundle-id <id>` — case-insensitive exact match on bundle
  identifier (for example, `com.apple.Safari`)

### Content shape

- `--kind text|html|rtf|url|file|image|pdf|binary|other` — filter
  by clipboard content type. `file` means file URLs (Finder paths),
  not arbitrary files. `other` means mixed or empty snapshots.
- `--has-text`, `--has-url`, `--has-file-url`, `--has-image`,
  `--has-pdf` — presence flags with additive AND semantics

`--has-text` includes snapshots whose only searchable text comes from
ready OCR text.

### Size

- `--min-bytes <N>` / `--max-bytes <N>` — applied to the total
  snapshot byte count

## Pagination

Every list command accepts `--limit` (bounded 1-250, default 10) and
an opaque `--cursor` returned as `next_cursor` in a prior response.
`stats` is a single aggregate response, so it doesn't paginate:

```bash
clipmem search "git status" --format json --limit 10
clipmem search "git status" --format json --limit 10 \
  --cursor "<next_cursor>"
```

Cursors are opaque and tied to the active query, mode, and filters.
Changing any of those while paginating rejects the cursor. When a
response includes `"truncated": true` and a non-null `next_cursor`,
there are more rows.

## Search modes

`search` and `recall` accept `--mode auto|fts|literal`
(default: `auto`).

- **auto** — picks FTS or literal per query. Prefers literal matching
  for URLs, paths, bundle IDs, dotted identifiers, and shell-like
  fragments. Plain-text queries use FTS first.
- **fts** — strict SQLite FTS5. Use for boolean queries like
  `"launchctl" AND bootstrap`.
- **literal** — exact substring match. Use for punctuation-heavy
  strings like `50%`, `Co-Authored-By:`, or URL fragments.

Rules of thumb:

- Query contains `"`, `AND`, `OR`, `NOT` — use `--mode fts`
- Query contains `/`, `.`, `:`, `%`, or shell metacharacters — use
  `--mode literal`
- Short natural-language query — let `--mode auto` pick

## Next steps

- [Output formats](output-formats.md) — choose the right format for
  scripts, agents, and terminal use
- [Managing your archive](managing-your-archive.md) — restore,
  delete, and configure capture policy
- [Command reference](command-reference.md) — exhaustive flag-level
  reference for every command
