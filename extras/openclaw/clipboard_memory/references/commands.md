# Clipboard Memory Commands

Use `clipmem` in this order:

1. `clipmem recall` for the best answer plus alternatives.
2. `clipmem timeline` when the user cares about chronology, repetition, or "what happened when".
3. `clipmem search` when the user wants direct lexical matching or you need to drive a narrower query.
4. `clipmem get` when you already have a `snapshot_id` and need the exact nested record.

## Decision ladder

Use `clipmem recall` first for most agent requests:

```bash
clipmem recall "<query>" --format json --limit 5
```

No query, or the user means "what did I copy recently?":

```bash
clipmem recall --prefer-recent --hours 24 --format json --limit 5
```

Need the exact text, not a short surfaced answer:

```bash
clipmem recall "<query>" --format json --quote --full
```

Need all clipboard events in time order:

```bash
clipmem timeline --hours 24 --format json
clipmem timeline --app safari --hours 24 --sort desc --format json
```

Need a direct lexical query:

```bash
clipmem search "<query>" --format json --limit 10
clipmem search "<query>" --mode literal --format json
```

Need nested detail for one hit:

```bash
clipmem get <snapshot_id> --format json
```

## Recent vs timeline

- `clipmem recent` is snapshot-centric and deduplicated. Use it for "show me recent unique clipboard states".
- `clipmem timeline` is event-centric and chronological. Use it for "what did I copy today", repeated copies, or source/time slices.

If the user says "today", "yesterday", "in order", "every time", or "from Safari today", prefer `timeline` or `recall` with time/source filters before `recent`.

## Common recipes

What was that command I copied?

```bash
clipmem recall "command I copied" --format json --limit 5
```

Show me things I copied from Safari today:

```bash
clipmem timeline --app safari --hours 24 --format json
```

Find the URL I copied yesterday:

```bash
clipmem recall "url" --has-url --hours 48 --format json --limit 5
```

Give me the exact text, not just a summary:

```bash
clipmem recall "<query>" --format json --quote --full
```

If `recall` identifies a likely hit and you need all stored detail:

```bash
clipmem get <snapshot_id> --format json
```

## Useful flags

- `--format json` for structured agent output.
- `--quote` to surface literal best text when available.
- `--full` to expand the best surfaced text.
- `--app <name>` to bias or filter by source app.
- `--since <RFC3339>`, `--until <RFC3339>`, and `--hours <N>` for time windows.
- `--has-url`, `--has-file-url`, `--has-text`, `--kind ...` to constrain content shape.

## What to read from the result

Prefer these fields first:

- `best_candidate.best_text`
- `best_candidate.urls`
- `best_candidate.file_paths`
- `why_selected`
- `alternatives`

Only walk nested `items[].representations[]` after `clipmem get` if the surfaced fields are not enough.
