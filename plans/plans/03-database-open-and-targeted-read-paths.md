# Plan 3 — Database open modes and targeted read paths

**Priority:** P0  
**Primary owners:** database core/schema, snapshot read layer, CLI retrieval/mutation, Swift client consumers  
**Depends on:** none; coordinate schema readiness with plan 1

## Problem and evidence

- `open_existing` is read-write and performs the same schema preparation as initialization (`src/db/core.rs:24-76`, `src/db/core.rs:224-227`).
- Preparation starts an immediate transaction and executes the full schema (`src/db/schema.rs:16-52`).
- `find_snapshot`/`snapshot_projection` load all BLOBs (`src/db/read/snapshot.rs:18-60`, `snapshot.rs:280-357`).
- List output loops snapshot projections, creating N+1 queries (`src/cli/commands/retrieval_support.rs:29-51`).
- Export validates via full `find_snapshot`, then reads target bytes; forced output replacement is non-atomic (`src/cli/commands/archive_mutate.rs:24-61`, `archive_mutate.rs:154-197`).
- Native image detail magnifies these reads.

## Required outcome

Routine status/revision/search/recent/timeline/get operations open the database without DDL or write intent and never read raw BLOBs unless requested. Export reads one representation and replaces its destination atomically. Migration is explicit and typed.

## Scope

- Connection/open API split.
- Schema metadata/current-version gate.
- Explicit migration/init lifecycle.
- Metadata/manifest/payload read types and SQL.
- Set-based projection/document loads.
- Atomic export.
- Query tracing/performance tests.
- CLI error mapping for migration required/read-only failures.

Out of scope: canonical projection contents (plan 4), daemon (plan 7), source image redesign (plan 6).

## Design decisions

### 1. Three open modes

Implement:

```text
Database::open_read_only_current(path)
Database::open_read_write_current(path)
Database::open_or_init_and_migrate(path)
```

`open_read_only_current`:

- `SQLITE_OPEN_READ_ONLY | URI | NO_MUTEX` (or the project's justified threading flag);
- connection-local query pragmas only; do not set WAL/journal mode or execute schema;
- read application/archive identity and `PRAGMA user_version`;
- validate exact supported current version and required schema signature;
- return typed `DatabaseOpenError::MigrationRequired`, `NewerSchema`, `NotArchive`, `Missing`.

`open_read_write_current`:

- no create;
- configure busy timeout, FK, and safe connection pragmas;
- same current-version/schema gate;
- no schema DDL.

`open_or_init_and_migrate`:

- create if needed;
- acquire migration transaction/lock;
- apply baseline for new DB or ordered migration steps;
- validate invariants and commit;
- used by setup, explicit migrate, service upgrade startup, tests.

Do not automatically migrate from every CLI read. A mutating command may choose to return migration-required with guidance unless product policy explicitly allows lifecycle migration before mutation.

### 2. Archive metadata/signature

Add/validate:

- SQLite `application_id` or a metadata table magic value;
- stable archive instance UUID;
- schema version and optional projection readiness/version.

This prevents an unrelated SQLite DB with coincidental tables/version from being treated as clipmem.

### 3. Cost-specific types/APIs

Define types without `Vec<u8>`:

```text
SnapshotMetadata
SnapshotDocument/Projection
SnapshotItemManifest
RepresentationManifest { uti, kind, byte_len, raw_sha256, ... }
CaptureEventSummary
```

Payload API:

```text
RepresentationPayload { manifest, bytes/reader }
RestorePayloadPlan { all source representations }
```

Methods:

- `find_snapshot_metadata(id, event_limit)` — snapshot + events + manifests, no BLOB.
- `find_snapshot_document(id)` — canonical/legacy projection, no BLOB.
- `find_snapshot_documents(ids)` — set-based bulk load preserving caller mapping.
- `find_representation_manifest(id,item,uti)` — no BLOB.
- `read_representation_payload(id,item,uti)` — exactly one BLOB.
- `load_restore_payload(id)` — intentionally all BLOBs and validates item/rep integrity.

Prevent accidental BLOB selection by keeping payload query SQL in a separate module and not exposing raw `ClipboardItem` construction to metadata paths.

### 4. CLI command mapping

- Search/recent/timeline/recall use rows + one bulk document query, or directly select needed document fields.
- `get` uses metadata/manifests and preserves current JSON omission of raw bytes.
- `export` queries target manifest/payload directly; no full snapshot validation.
- `restore` uses `load_restore_payload` explicitly.
- `service revision`, settings show, status DB reads, and doctor read checks use read-only current connection where possible.
- Stats uses read-only current except temp tables; SQLite TEMP writes are connection-local and can work with read-only main DB—verify. If a pragma/query requires write, use an explicit reasoned path.

### 5. Atomic export

Algorithm:

1. Validate destination parent and existing target type/symlink policy without deleting it.
2. Create a unique temp file in the same directory with exclusive create.
3. Stream/write exactly one payload.
4. Flush and `sync_all` if current durability expectation warrants it.
5. Apply desired permissions.
6. Atomically rename/replace target. Use platform-correct replacement semantics; on macOS/Unix, ensure rename-over-existing behavior and reject unsafe target types as currently intended.
7. Sync directory if claiming crash durability.
8. Clean temp file on every failure.

Return output only after replacement succeeds.

### 6. Busy/transaction policy

After removing read write-intent, reevaluate the 1.5-second busy timeout. Use:

- short/read-only timeout with clear retryable error;
- longer bounded write/migration timeout or explicit retry strategy;
- no long image/OCR work inside write transactions.

Do not globally raise timeout to hide lock design.

## Implementation sequence

1. Add connection trace/test hooks that record statements and BLOB-column access in tests.
2. Add typed open errors and archive metadata migration.
3. Implement three open methods; retain old methods as deprecated wrappers temporarily with explicit semantics.
4. Change setup/migration tests and service setup to use migrate mode.
5. Convert revision/settings/search/list/get/status reads to read-only/current.
6. Introduce metadata/manifest/payload types and queries.
7. Convert list projection to set-based query; use plan 1 unified document when available, otherwise bulk legacy caches without BLOBs.
8. Convert get/export/restore call sites.
9. Implement atomic export and failure-injection tests.
10. Delete or make private BLOB-hydrating generic methods; names must advertise payload cost.
11. Profile CLI/app fixture with large image/PDF snapshots and save before/after results.
12. Update docs/error guidance for migration-required state and add an explicit `clipmem migrate` command if setup/service startup is not sufficient.

## Edge cases and failure modes

- Read-only file/database directory must support read operations without WAL-mode mutation. Test existing WAL/SHM behavior and immutable/read-only URI modes carefully.
- A DB one version behind returns migration-required; it must not partially operate.
- Newer schema returns a distinct error and never writes.
- New database setup remains atomic; failed migration leaves old version usable/clearly recoverable.
- Metadata query must handle snapshots with zero items only if schema/product permits them.
- Target representation missing distinguishes snapshot missing, item missing, and UTI missing where output contract benefits.
- Export cross-filesystem issue is avoided by temp file in destination directory.
- Rename behavior with existing symlink/directory remains rejected.
- Large payload streaming should avoid an unnecessary second full copy where rusqlite/API permits; correctness first, then memory optimization.

## Tests

- Read-only open on chmod/read-only fixture performs no write statement and does not create `-wal`/`-shm` unexpectedly.
- SQL trace asserts no `BEGIN IMMEDIATE`, `CREATE`, `ALTER`, `PRAGMA user_version=` on current read.
- Migration-required/newer/not-archive/missing typed errors and CLI exit mapping.
- Get metadata for a 100 MB BLOB fixture with a query/allocator contract proving BLOB not selected.
- List 40 large snapshots executes bounded query count (target ≤3 archive queries after open) and no BLOB select.
- Export selects one BLOB; write failure preserves previous target bytes; successful force replace is atomic.
- Restore intentionally reads all representations once.
- Concurrent revision polling and writer no longer contend on schema transaction.

## Acceptance criteria

- `service revision`, search, recent, timeline, recall, get, settings show, and read-only status do not execute schema SQL or acquire write transaction.
- Metadata/list/get paths do not select `blob_value`.
- Search list projection is set-based, not one query per result.
- Export is failure-atomic and reads one target representation.
- All supported migration fixtures upgrade through explicit migrate mode.
- Large-payload benchmark shows substantial reduction in bytes read and memory; report actual figures rather than claiming a target without measurement.

## Rollout and rollback

Keep old `open_existing` wrapper for one release, instrument/log test-only use, and migrate call sites incrementally. Schema metadata additions are backward-compatible. If read-only mode exposes platform WAL issues, fall back only the affected command to read-write/current (still no schema preparation), document why, and retain typed separation.
