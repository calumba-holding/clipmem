# Clipboard Memory Commands

Use `clipmem` in this order:

1. `clipmem recall` for the best answer plus alternatives.
2. `clipmem timeline` when chronology matters.
3. `clipmem search` for direct lexical matching.
4. `clipmem get` for nested detail after you have a `snapshot_id`.

## Quick recipes

```bash
clipmem recall "<query>" --format json --limit 5
clipmem recall --prefer-recent --hours 24 --format json --limit 5
clipmem recall "<query>" --format json --quote --full
clipmem timeline --app safari --hours 24 --format json
clipmem search "<query>" --format json --limit 10
clipmem get <snapshot_id> --format json
```

## Recent vs timeline

- `recent` is deduplicated by snapshot.
- `timeline` shows actual capture events in order.

Prefer `timeline` for "today", "yesterday", "every time", or source/time slicing.
