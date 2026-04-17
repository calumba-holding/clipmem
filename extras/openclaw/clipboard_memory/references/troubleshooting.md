# Clipboard Memory Troubleshooting

## No result or weak result

If `clipmem recall` returns weak or empty results:

1. widen the time window with `--hours`, `--since`, or `--until`
2. add or remove source filters like `--app safari`
3. switch to `clipmem timeline` if the user cares about chronology
4. switch to `clipmem search` for a more literal query

Examples:

```bash
clipmem recall "<query>" --hours 72 --format json
clipmem timeline --hours 72 --format json
clipmem search "<query>" --mode literal --format json
```

## When to switch commands

- Use `recall` when the user wants the likely answer.
- Use `timeline` when the user asks "today", "yesterday", "in order", or "every time".
- Use `search` when you need exact lexical matching.
- Use `get` when you already have a `snapshot_id` and need nested detail or provenance.

## Host PATH and sandbox issues

If `clipmem` is installed but OpenClaw cannot run it:

1. run `clipmem agents openclaw doctor`
2. confirm `clipmem` is on the host PATH
3. if sandboxing is active, confirm `clipmem` is also available inside the sandbox

Useful commands:

```bash
clipmem agents openclaw doctor
openclaw sandbox explain
```

If the binary was installed after sandbox creation, recreate the sandbox containers and retry.

## Binary, image, and PDF clipboard items

Some snapshots have no safe text projection. If the hit is image-only, PDF-only, or binary-only:

- report the metadata you do have
- mention that `clipmem export` is required for raw bytes
- do not pretend exact text exists when it was never captured as text

## Exact text unavailable

If the user asks for exact text and `best_text` is empty or clearly partial:

- say that the stored surfaced text is incomplete or unavailable
- offer the closest metadata-backed answer
- use `clipmem get <snapshot_id> --format json` for more detail
- mention `clipmem export` if the content appears to be binary-only

Good wording:

- "I found the clipboard item, but clipmem did not capture usable text for it."
- "I can show the metadata and source app, but exact text recovery would require a raw export."
