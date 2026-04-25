---
title: Improve File URL Capture Storage Performance
date: 2026-04-24
category: docs/solutions/performance-issues/
module: clipboard capture storage
problem_type: performance_issue
component: database
symptoms:
  - "Storing a new clipboard snapshot with many file URL representations was slow."
  - "A 1,000-file-url snapshot store benchmark had a median runtime of 3.565 s before the fix."
  - "Representation triggers rebuilt snapshot projection and literal-search caches once per representation row."
root_cause: logic_error
resolution_type: code_fix
severity: high
related_components:
  - store_capture
  - item_representations triggers
  - snapshot_projection_cache
  - snapshot_literal_cache
  - capture_events trigger
tags:
  - clipboard-capture
  - file-urls
  - sqlite-triggers
  - representation-cache
  - projection-cache
  - literal-cache
  - schema-migration
  - performance
---

# Improve File URL Capture Storage Performance

## Problem

Storing clipboard snapshots with many file URL representations was dramatically slower than reading or searching existing snapshots. The bottleneck appeared on the write path for Finder-style captures: storing a 1,000-file-url snapshot took a median 3.565 s before the fix, while the relevant `search_literal` path in the existing large-archive profile was around 5.6 ms.

## Symptoms

- Capturing a new snapshot containing many file URL representations was visibly slow.
- Retrieval profiling did not show a comparable bottleneck; `search_literal` was already in the single-digit millisecond range on the large retrieval harness.
- The targeted write benchmark showed poor scaling for high-representation snapshots:
  - Before: 3.565 s median for a 1,000-file-url capture.
  - After: 52.203 ms median for the same capture.
- The slowdown was tied to newly inserted snapshots with many `item_representations` rows.

## What Didn't Work

- Optimizing retrieval was not the right target. The existing large-retrieval profile showed `search_literal` at about 5.6 ms, so further read-side tuning could not explain multi-second capture latency.
- Startup and `open_existing` checks were also not dominant. The existing startup profile was in the low-millisecond range and did not match the 1,000-file-url store benchmark.

## Solution

Defer representation-derived cache work while inserting representation rows for a newly inserted snapshot, then rebuild the snapshot projection cache once after all rows are present.

The fix added an internal deferral flag to `clipmem_settings`:

```sql
representation_cache_deferred INTEGER NOT NULL DEFAULT 0
    CHECK (representation_cache_deferred IN (0, 1))
```

The `item_representations` triggers now skip their expensive cache-maintenance bodies while that flag is enabled:

```sql
DROP TRIGGER IF EXISTS item_representations_ai;
CREATE TRIGGER item_representations_ai AFTER INSERT ON item_representations
WHEN NOT EXISTS (
    SELECT 1 FROM clipmem_settings
    WHERE id = 1 AND representation_cache_deferred = 1
)
BEGIN
    -- snapshot_projection_cache and snapshot_literal_cache maintenance
END;
```

The update and delete triggers use the same guard. `schema.sql` uses `DROP TRIGGER IF EXISTS` followed by `CREATE TRIGGER` so existing databases receive the updated trigger bodies during schema preparation.

`store_capture` now wraps only the new-snapshot representation insert path with the deferral flag:

```rust
let snapshot_id = if let Some(id) = inserted_snapshot_id {
    set_representation_cache_deferred(&tx, true)?;
    for item in snapshot.items() {
        insert_item(&tx, id, item)?;
    }
    rebuild_snapshot_projection_cache_for_snapshot(&tx, id)?;
    set_representation_cache_deferred(&tx, false)?;

    id
} else {
    tx.query_row(
        "SELECT id FROM snapshots WHERE sha256 = ?1",
        [snapshot.fingerprint()],
        |row| row.get(0),
    )?
};
```

Because this runs inside the capture transaction, an error rolls back the flag change with the rest of the insert.

The capture event is inserted after the deferral flag is cleared. That preserves the existing `capture_events` trigger behavior for snapshot stats, event-filter cache, and literal-search cache refreshes using complete app metadata.

The schema version moved from 13 to 14, and `ensure_representation_cache_deferred_column` adds the internal setting column for older databases.

Tests added:

- `deferred_representation_cache_keeps_file_urls_searchable` verifies that file URLs remain searchable after the deferred path and that `representation_cache_deferred` resets to `0`.
- `schema_version_14_adds_representation_cache_deferral_column` verifies migration of the setting column and replacement of an older insert trigger body.

Verification run:

```sh
cargo test
cargo fmt --check
python3 scripts/check_file_lengths.py
cargo clippy --all-targets --all-features -- -D warnings
```

## Why This Works

The old trigger behavior rebuilt representation-derived cache state after every inserted `item_representations` row. For a snapshot with many file URLs, each insert repeatedly aggregated an increasingly large set of rows for the same snapshot.

That put snapshot-level work inside a per-row trigger path. A 1,000-representation capture could therefore perform hundreds or thousands of partial cache rebuilds before the snapshot was complete.

The deferral flag changes only the bulk insert window:

1. `store_capture` sets `representation_cache_deferred = 1`.
2. Snapshot item and representation rows are inserted without running the expensive representation-trigger bodies.
3. `rebuild_snapshot_projection_cache_for_snapshot` rebuilds projection cache state once for the complete snapshot.
4. `store_capture` clears the deferral flag.
5. The following `capture_events` insert refreshes the remaining event-derived and literal-search caches with final app metadata.

This preserves final cache contents while replacing repeated partial rebuilds with one complete rebuild. The benchmark improved from 3.565 s median to 52.203 ms median for the same 1,000-file-url capture, about 68x faster.

## Prevention

- Benchmark the path that matches the symptom. Retrieval profiles were useful context here, but the user-visible slowdown came from storing a new capture.
- Avoid row-level triggers for work that aggregates across all representations in a snapshot when common writes insert many rows at once.
- When trigger-maintained state is still useful for steady-state updates, provide an explicit batch deferral path and a single rebuild after the batch completes.
- Keep internal deferral flags inside the same transaction as the batched write, and cover the completed path with reset assertions so cache maintenance is not left disabled.
- When changing SQLite trigger bodies, use explicit replacement in schema setup and migration tests. `CREATE TRIGGER IF NOT EXISTS` will not update an older trigger body.

## Related Issues

- No related GitHub issue was found for this performance problem.
- `CHANGELOG.md` contains the release-note summary and benchmark numbers.
- `docs/architecture.md` documents the trigger-maintained cache model behind this fix.
