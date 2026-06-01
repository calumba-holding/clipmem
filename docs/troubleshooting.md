# Troubleshooting

Diagnose before reinterpreting. Most "nothing found" outcomes are a
stale watcher or a mismatched filter, not a true miss.

Start by running diagnostics:

```bash
clipmem doctor
clipmem service status
```

## Empty or weak recall results

Work through these in order, stopping when the result improves:

1. **Widen the time window.** Use `--hours 72`, or drop `--hours`
   entirely.
2. **Remove source filters.** Your memory of which app doesn't always
   match what clipmem recorded as the frontmost process.
3. **Switch to `timeline`.** Chronological order with filters often
   finds things that `recall`'s ranker misses.
4. **Switch to `recent`.** Deduplication can reduce noise when you
   want unique items.
5. **Switch to `search`.** For exact phrases or punctuation-heavy
   strings, try `--mode literal`.
6. **Loosen content shape.** Drop `--kind`, `--has-url`, `--has-text`
   flags — they may be excluding the right snapshot.

```bash
clipmem recall "<query>" --hours 72 --format json
clipmem timeline --hours 72 --format json --limit 25
clipmem recent --hours 72 --format json --limit 25
clipmem search "<query>" --mode literal --format json
```

## Watcher not running

Symptom: `clipmem timeline --hours 1` returns zero rows despite having
copied recently.

Check the watcher state:

```bash
clipmem service status --json
```

If `stale: true` or neither the Homebrew service nor the direct
LaunchAgent is running:

```bash
clipmem setup
```

If `watcher_binary_mismatch: true`, the command you ran and the
background watcher are using different `clipmem` binaries. Restart the
watcher from the same binary with `clipmem service start`, or use
`scripts/build_and_run_menubar.sh` for development so the menu bar app
and watcher use `target/debug/clipmem` together.

## Doctor says everything is fine but you get no results

Two common causes:

- **Polling gap.** The watcher polls every 50ms by default. If the
  clipboard changed multiple times within a single poll window,
  intermediate states can be missed. This is rare in practice but can
  happen with rapid clipboard automation.
- **Copied before the watcher started.** The watcher only captures
  clipboard changes it observes while running. It doesn't
  retroactively index clipboard contents from before it was started.
  Run `clipmem setup` to ensure the watcher is running, then copy the
  item again.

## FTS mode failures

`clipmem search "..." --mode fts` fails with a syntax error:

- Punctuation in the query confuses FTS5. Switch to `--mode literal`.
- Check `clipmem doctor --json` for
  `"fts5_create_virtual_table_ok": true`. If false, the SQLite build
  lacks usable FTS5 and every `--mode fts` call fails.
- Mix of quotes and operators (`"foo" AND bar`) should parse. Unbalanced
  quotes don't.

## Binary-only snapshots

Symptom: `best_text` is `null` or empty, yet `total_bytes > 0` and
`kind` is `image`, `pdf`, or `binary`.

This is expected. Those clipboard states have no safe text projection.
To recover the content:

1. Inspect the snapshot detail:

```bash
clipmem get <snapshot_id> --format json
```

2. Find the right UTI in `items[].representations[]` (for example,
   `public.png`, `com.adobe.pdf`).

3. Export the raw bytes:

```bash
clipmem export <snapshot_id> --item <index> --uti <uti> --out <path>
```

## Sandbox and PATH issues

Symptom: an agent runtime can't execute `clipmem` even though it
runs fine in your interactive shell.

1. Confirm `clipmem` is on the agent's PATH, not just your shell
   PATH.
2. If sandboxing is active (OpenClaw containers, Apple sandbox, and
   similar), both the binary and the SQLite file at
   `~/Library/Application Support/clipmem/clipmem.sqlite3` must be
   visible inside the sandbox.
3. If the binary was installed after the sandbox was created, the
   sandbox image may need to be recreated.

If available, `openclaw sandbox explain` is the fastest way to see
the visible PATH and file-access scope.

## Locked or corrupt database

Symptom: `clipmem doctor --json` includes errors, or retrieval
commands exit with code `5`.

Run diagnostics:

```bash
clipmem doctor --json
```

Common issues:

- **"database is locked"** — another writer is holding the lock.
  Usually the watcher under heavy load. Try again in a few seconds.
- **"incompatible prerelease schema"** — an older archive format is
  being used. Move the file aside and run `clipmem setup`.
- **"malformed" or "corrupt"** — SQLite detected structural damage.
  Back up the database, delete it, and let `clipmem setup` rebuild.
  You'll lose history.
- **"permission denied"** — the database file isn't writable by the
  current user. Check `0600` on the file and `0700` on the
  containing directory.

## Exit code reference

| Code | Meaning | Typical response |
|---|---|---|
| `0` | Success | Continue |
| `1` | Uncategorized runtime failure | Inspect stderr, try again |
| `2` | Invalid arguments | Fix the command flags |
| `3` | Not found | Try a different snapshot ID or query |
| `4` | Unsupported format | Wrong `--format` for this command |
| `5` | Database error | See "Locked or corrupt database" above |
| `6` | Platform error | macOS API or filesystem issue |

## Next steps

- [Getting started](getting-started.md) — set up background capture
- [Command reference](command-reference.md) — exhaustive flag-level
  reference
- [Privacy and security](privacy-and-security.md) — understand data
  storage and cleanup
