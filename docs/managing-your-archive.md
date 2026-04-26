# Managing your archive

clipmem stores the full per-item representation set for every clipboard
snapshot, not just the best text. This lets you restore snapshots
faithfully, export raw binary payloads, and manage your archive with
fine-grained deletion and retention policy.

## Restore a snapshot

`restore` puts a stored snapshot back onto the macOS clipboard as a
whole snapshot — not a text-only approximation. It rehydrates every
item and representation (images, URLs, RTF, file URLs, and more).

```bash
clipmem restore 42
clipmem restore 42 --format json
```

The active watcher suppresses the matching restored state, so restoring
an archived snapshot doesn't create a duplicate capture event.

`restore` is macOS-only.

## Export raw bytes

`export` writes the raw bytes for a single representation to disk. Use
it to recover binary payloads like images and PDFs.

```bash
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png
clipmem export 42 --item 0 --uti public.png --out ./clipboard.png \
  --force
```

- `--item` — 0-based item index inside the snapshot
- `--uti` — the representation UTI to export (for example,
  `public.png`, `public.utf8-plain-text`, `com.adobe.pdf`)
- `--out` — destination path for the raw bytes
- `--force` — replace an existing regular file at the destination

By default, `export` creates a new file and refuses to replace an
existing path. Symlink destinations are never allowed.

To find the right `--item` and `--uti` combination, inspect the
snapshot first:

```bash
clipmem get 42 --format json
```

Look at `items[].representations[]` for the `uti` and `size_bytes`
of each representation.

## Delete a single snapshot

`forget` irreversibly deletes one snapshot, all its child
representations, and all capture events for that snapshot ID. There is
no recycle bin.

```bash
clipmem forget 42
clipmem forget 42 --format json
```

## Prune old snapshots

`purge` deletes snapshots whose last observation is older than a
given duration. It ages snapshots by `last_observed_at`, not the
original snapshot creation time.

```bash
clipmem purge --older-than 30d --dry-run
clipmem purge --older-than 30d
clipmem purge --older-than 12h --dry-run --format json
```

Duration grammar is a single integer plus one unit: `Nd` (days),
`Nh` (hours), or `Nm` (minutes). Compound durations like `1h30m`
aren't supported.

Use `--dry-run` to preview what would be deleted without deleting
anything.

## Compact database storage

`storage compact` asks SQLite to checkpoint and vacuum the database.
It reclaims free database and WAL pages back to the filesystem and
does not change clipboard content.

```bash
clipmem storage compact --dry-run --format json
clipmem storage compact
```

Use the dry run first if you want to inspect current DB/WAL/SHM file
sizes, page count, freelist count, and checkpoint state. A real compact
may need temporary disk space while SQLite rebuilds the database.

## Optimize stored images

`storage optimize-images` rewrites eligible stored image
representations as lossless WebP when doing so saves meaningful space,
then compacts SQLite storage by default so freed pages are returned to
the filesystem. Exact decoded pixels are preserved, including alpha,
but original encoded container metadata is not preserved.

```bash
clipmem storage optimize-images --dry-run --format json
clipmem storage optimize-images --limit 50 --format json
clipmem storage optimize-images --no-compact --limit 50 --format json
clipmem storage optimize-images --progress jsonl
```

Only image rows marked `uncompressed` are considered. Rows converted
to WebP are marked `compressed`, and rows that are unsupported,
corrupt, animated, not smaller, or conflict with existing archive rows
are marked `skipped`. Normal optimization runs never retry rows that
have already been compressed or skipped. Use `--no-compact` only when
you want to batch several limited optimization runs and compact once at
the end.

## Capture policy

Persistent capture policy lives in SQLite and is managed through
`clipmem settings`. These settings survive restarts and apply to
all capture methods (background watcher, foreground watch, and
capture-once).

### View current policy

```bash
clipmem settings show
clipmem settings show --format json
```

### Pause and resume capture

Persistently pause capture without stopping the watcher service:

```bash
clipmem settings pause on
clipmem settings pause off
```

### API key filter

Opt into a precision-biased secret guard that skips clipboard
snapshots matching API key or token patterns. Matching snapshots
are skipped entirely — no preview text, search text, or blob
payloads are written.

```bash
clipmem settings api-key-filter on
clipmem settings api-key-filter off
```

The filter is heuristic and tuned to avoid false positives rather
than catch every possible secret.

### Retention

Set automatic retention to prune old snapshots, or disable it:

```bash
clipmem settings retention 30d
clipmem settings retention forever
```

Retention ages snapshots by `last_observed_at`, not the original
insertion time.

### Ignore list

Exclude specific apps from capture by their bundle ID. Matching is
exact and case-insensitive.

```bash
clipmem settings ignore add com.apple.Passwords
clipmem settings ignore remove com.apple.Passwords
clipmem settings ignore list
clipmem settings ignore list --format json
```

## Next steps

- [Searching and filtering](searching-and-filtering.md) — find
  clipboard items with recall, search, timeline, and filters
- [Privacy and security](privacy-and-security.md) — understand how
  your data is stored and protected
- [Command reference](command-reference.md) — exhaustive flag-level
  reference for every command
