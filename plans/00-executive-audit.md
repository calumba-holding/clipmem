# Executive audit

## Bottom line

Clipmem has a sound **product-level core model**: immutable-looking clipboard snapshots composed of ordered items and UTI-keyed representations, with separate capture events so repeated copies deduplicate payloads while retaining history. That is the part to preserve.

The weakest part is not the domain model; it is the system around it. Policy is reimplemented by entry point, routine reads perform schema work with write intent, expensive BLOBs are loaded for metadata-only operations, background jobs have no durable ownership, search combines incompatible ranking streams with an unsafe cursor, and a large trigger network silently maintains several overlapping projections. The native app magnifies those costs by spawning a CLI process per request and polling every two seconds.

A big-bang rewrite is not warranted. The highest-return route is:

1. Lock down retrieval and capture/restore correctness.
2. Separate migration, read-only, and read-write database opens.
3. Add targeted metadata/payload APIs.
4. Establish one canonical projection contract and enforce relational integrity.
5. Add a durable job protocol.
6. Stop image optimization from changing source identity.
7. Only then decide, from measurements, whether a long-lived local service should replace high-frequency subprocess calls.

## Most important confirmed defects

### 1. Recall reports every FTS hit as a perfect match

SQLite FTS5 `bm25()` returns lower-is-better negative values in this code's query shape. `normalize_fts_score` clamps all negative values to zero before computing `1 / (1 + value)`, so every FTS result becomes `1.0` (`src/cli/commands/retrieval/recall.rs:370-373`). An executable FTS5 probe produced distinct ranks such as `-1.419e-6` and `-1.113e-6`; the current transform maps all of them to the same score.

This is not cosmetic. It disables the weak-match branch (`recall.rs:155-203`), inflates confidence, prevents intended expansion/recent fallback whenever any FTS row exists, and leaves ordering to unrelated bonuses and original rank. Fix this before tuning recall heuristics.

### 2. Native-text and OCR search cannot be paginated safely with one cursor

Native and OCR indexes are queried independently, each with `limit + 1`, then merged and truncated (`src/db/read/search.rs:223-279`, `src/db/read/search_results.rs:15-62`). A cursor derived from the merged last row is applied separately to both independent score spaces on the next request. Duplicates are removed only after both sources have already paginated, and a duplicate always retains native match explanation rather than combining provenance.

This can skip unseen results across pages and can misstate why a row matched. A unified candidate stream or a source-aware composite cursor is required.

### 3. Capture behavior changes depending on how capture was invoked

The watcher applies pause and ignored-app policy, watched-capture suppression, OCR enqueue/work, retention, and notifications (`src/cli/commands/runtime.rs:32-159`). `capture-once` uses the watched store path and retention/notification, but does not check pause/ignored-app settings and does not enqueue OCR (`src/cli/commands/runtime.rs:206-252`). Setup seed capture uses `store_capture_if_allowed`, bypassing restore suppression and the other watcher orchestration (`src/cli/service/manage.rs:236-249`). Documentation says persistent policy applies to all capture methods (`docs/managing-your-archive.md:125-128`).

One application service should own the policy matrix and transaction boundary.

### 4. Restore suppression and restore itself have race/failure windows

Watched capture consumes the pending restore marker in its own transaction before storing (`src/db/store/capture.rs:132-190`). A second watcher can then store the same restored state. The schema trigger is also deliberately one-shot (`src/db/schema.sql:314-324`).

Separately, macOS restore clears the live pasteboard before all destination items have been constructed (`src/platform/macos.rs:63-92`). An unsupported or invalid representation can therefore destroy the user's current clipboard and still return an error.

Restore must prepare first, write second, and use a generation/change-count-aware suppression token that remains valid for all legitimate watcher contenders.

### 5. Routine reads take write intent and repeatedly execute schema setup

Both `open_or_init` and `open_existing` use read-write SQLite flags and call the same preparation path (`src/db/core.rs:24-76`, `src/db/core.rs:224-227`). That path starts an `IMMEDIATE` transaction, executes the full 858-line schema, checks migrations, and writes `user_version` when needed (`src/db/schema.rs:16-52`). The native app invokes a fresh CLI process for every read and polls `service revision` every two seconds (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:575-636`).

The result is avoidable lock contention, startup work, and fragile behavior during maintenance. Migration must become an explicit/open-time gate, while current-schema reads use truly read-only connections.

### 6. Metadata reads load raw payloads

`find_snapshot` and `snapshot_projection` hydrate all representation BLOBs through `load_snapshot_items` (`src/db/read/snapshot.rs:18-60`, `src/db/read/snapshot.rs:280-357`). List commands then call projection loading once per result (`src/cli/commands/retrieval_support.rs:29-51`). `get` serializes representations without raw bytes, and `export` first calls `find_snapshot` and then reads the requested bytes again (`src/cli/commands/archive_mutate.rs:24-61`).

In the native image detail flow, the app runs `get`, then `export` to a temporary file (`macos/ClipmemMenuBar/ClipmemMenuBar/Views/SnapshotDetailView.swift:222-264`), causing repeated all-payload reads for one preview. Introduce metadata-only snapshot reads and a direct targeted payload stream/file API.

### 7. The database permits representation rows for nonexistent items

`item_representations` has a foreign key only to `snapshots`, not to `(snapshot_id, item_index)` in `snapshot_items` (`src/db/schema.sql:25-41`). An executable probe inserted an orphan representation at item index 99; `PRAGMA foreign_key_check` reported no violation, and the normal item loader silently omitted it. Add a composite foreign key and a migration that detects/repairs or quarantines existing orphans.

### 8. Image optimization breaks exact-source and stable-identity semantics

The optimizer replaces the stored UTI, bytes, raw hash, source metadata, and snapshot fingerprint in place (`src/db/store/optimize.rs:532-664`). The README promises exact rich restore and archive documentation calls export the raw bytes (`README.md:24`, `docs/managing-your-archive.md:24-38`). A later capture of the original bytes can become a different snapshot because the optimized row now has a different identity.

This is an explicit trade-off in the current implementation, not merely an accidental line bug, but it is the wrong default for the product's stronger contract. Preserve source bytes/identity; store derivatives separately or use a reversible physical encoding below the logical representation layer.

## Architecture judgment

### What should stay

- Snapshot → ordered item → representation as the authoritative payload shape.
- Capture events separate from payload identity.
- Raw UTI representations as the source of truth.
- SQLite as the local archive.
- A CLI as a stable automation/agent surface.
- Revision categories as a useful invalidation concept, after mutation coverage is made complete.
- FTS and maintained projections, provided they are generated from one canonical document contract.

### What should be replaced or consolidated

- Per-command ad-hoc domain orchestration.
- Full schema preparation on every open.
- BLOB-hydrating metadata APIs.
- Five partially overlapping index/cache ownership paths with large mutation triggers.
- Independent native/OCR pagination with one cursor.
- Process-local “only one worker” assumptions.
- In-place source image mutation.
- High-frequency native-app subprocess polling as the eventual steady state.

### What should not be over-engineered yet

- Do not begin with a daemon rewrite.
- Do not replace SQLite.
- Do not introduce a generic repository/ORM layer over simple SQL.
- Do not merge all CLI command parsing and rendering into domain services; preserve clear command adapters.
- Do not redesign agent skill packaging unless a concrete parity failure exists.
- Do not rewrite SwiftUI views merely to adopt a new pattern; fix state and transport contracts first.

## Priority and effort honesty

| Priority | Work | Value | Relative effort | Why now |
|---|---|---:|---:|---|
| P0 | Retrieval correctness | Very high | Medium | Current confidence and pagination can be wrong. |
| P0 | Capture/restore consistency | Very high | Medium-high | Prevents lost clipboard state, policy bypass, and duplicate restore captures. |
| P0 | Database open modes + targeted reads | Very high | Medium | Removes systemic contention and the dominant read amplification. |
| P1 | Schema integrity + canonical projection | High | High | Simplifies load-bearing derived state and prevents silent corruption. |
| P1 | Durable jobs | High | Medium-high | Required for correct multi-process OCR/optimization and safe resumability. |
| P1 | Source-preserving image design | High | High | Aligns storage optimization with exact restore/identity promises. |
| P1/P2 gate | Long-lived service boundary | Potentially high | Very high | Valuable only after direct DB inefficiencies are fixed and measured. |
| P2 | Native app resilience/UX | Medium-high | Medium | Important user experience work, but some problems disappear with earlier APIs. |
| P2 | Projection quality | Medium | Medium-high | Better search/detail fidelity; lower urgency than correctness. |
| P2 | Hygiene/docs/release | Medium | Low-medium | Keeps future agents from repeating architectural drift. |

## Recommended delivery sequence

Implement plans 1–3 as independent but coordinated P0 work. Plan 3 should introduce the open/read APIs that plans 4, 6, and 8 will consume. Then execute plan 4 before changing search storage, plan 5 before running more than one worker, and plan 6 before advertising image optimization as source-safe. Treat plan 7 as a measured architecture gate rather than a foregone conclusion.

The plans are written so a weaker implementation model does not need to rediscover these decisions.
