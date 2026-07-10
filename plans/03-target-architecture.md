# Target architecture

## Design principles

1. **Source payloads are immutable.** Derived text, OCR, previews, indexes, and storage encodings may change; source identity and exact logical bytes do not.
2. **One application service owns each domain sequence.** CLI, watcher, setup, and app call it rather than assembling policy and transactions themselves.
3. **Migration is lifecycle work.** Normal reads do not execute DDL or take write intent.
4. **Cost is explicit in APIs.** Metadata methods cannot accidentally return BLOBs.
5. **Derived state has one builder and a version.** Rebuildability is a feature, not an incidental migration path.
6. **A rank is not confidence.** Retrieval output distinguishes raw/internal rank, evidence, and user-facing confidence.
7. **Jobs are claimed, leased, and idempotent.** Multi-process behavior is designed, not prevented by convention.
8. **Keep the CLI contract.** A future service is an additional transport, not a replacement for automation/agents.
9. **Prefer incremental migration over a rewrite.** Each step must leave a working archive and compatible CLI.

## Proposed logical layers

```text
Adapters
  CLI commands | watcher loop | Swift app transport | agent commands
        │
Application services
  CaptureService | RetrievalService | SnapshotService | RestoreService
  SettingsService | MaintenanceService | JobService
        │
Domain
  Snapshot source model | Capture policy | Projection document
  Search evidence/ranking | Job state machine | revision events
        │
Persistence/platform ports
  ArchiveReader | ArchiveWriter | PayloadStore | JobRepository
  PasteboardReader/Writer | OCR engine | image derivative encoder
        │
SQLite + AppKit/Vision
```

This does **not** require a framework-heavy dependency-injection system. Plain Rust structs with explicit references/transactions are sufficient.

## Data model

### Source archive

Keep the existing snapshot/item/representation/event concepts, with these changes:

- Add composite FK from representation to item.
- Add a stable archive instance ID in metadata for app configuration and migrations.
- Keep snapshot fingerprint based only on exact logical source representations.
- Store observed frontmost app on capture events; optional content-origin fields are separate.
- Add source representation content version only if exact bytes can legitimately be repaired; normal maintenance must not mutate it.

### Canonical snapshot document

Replace overlapping ad-hoc projections with a versioned `snapshot_documents` row (name illustrative) built from source manifest + event aggregate + OCR facts:

- `snapshot_id`, `builder_version`, `source_fingerprint`
- canonical best text and UTI
- structured text fragments (normalized child table or compact JSON)
- HTML/RTF extracted text
- distinct URLs and file paths
- distinct historical app names/bundle IDs if that is the chosen search contract
- capability flags: native text, OCR text, URL, file URL, image, PDF, binary
- summary/preview fields intended only for display
- last-observed/event aggregate fields needed by browse/filter
- `document_revision`/updated timestamp

FTS tables become indexes over this document, not competing owners of meaning. Native and OCR can be separate FTS columns in one row so one query returns one snapshot with field-specific evidence. If tokenizer constraints require multiple virtual tables, a unified candidate CTE must dedupe and rank before pagination.

### Payload and image derivatives

Keep source representation bytes logically exact. Add a derivative table:

```text
representation_derivatives
  id
  snapshot_id, item_index, source_uti, source_raw_sha256
  kind                 -- preview / storage-rendition / thumbnail
  codec, algorithm_version
  byte_len, raw_sha256, blob/storage locator
  width, height, metadata needed by decoder
  status, created_at
  UNIQUE(source_raw_sha256, kind, algorithm_version)
```

For storage reclamation there are two acceptable designs:

1. **Safest first:** retain source BLOB and add smaller preview derivatives. This improves UI but not archive size.
2. **Exact reversible storage encoding:** store compressed physical bytes plus encoding metadata, and reconstruct exact original bytes for restore/export/fingerprint. The logical representation API must return the original bytes byte-for-byte. This is more work and should be justified by measured savings.

Do not call a pixel-equivalent WebP replacement “exact source preservation.”

### Durable jobs

A generic but small job table supports OCR and derivatives:

```text
jobs
  id, kind, dedupe_key, algorithm_version
  status: queued | leased | succeeded | failed | skipped | cancelled
  priority, created_at, not_before
  lease_owner, lease_until
  attempts, max_attempts, last_error
  payload_ref/result_ref
  operation_id
  updated_at
UNIQUE(kind, dedupe_key, algorithm_version)
```

Claims are atomic and ordered. Completion verifies lease owner/version. Expired leases requeue. Domain result tables remain authoritative; jobs represent execution.

## Database connection lifecycle

### `open_read_only_current`

- SQLite read-only flags.
- Apply connection-local safe pragmas only.
- Read `application_id`/archive metadata and `user_version`.
- Fail with a typed `MigrationRequired { found, supported }` without DDL.

### `open_read_write_current`

- Read-write, no create.
- Same current-version gate.
- No schema SQL replay.
- Used for ordinary capture/settings/mutations.

### `open_or_init_and_migrate`

- Used only by setup, explicit migrate/doctor-repair, service startup upgrade path, and tests.
- Acquires migration lock/transaction, performs versioned migration steps, validates invariants, records migration history.

Schema creation should be a baseline plus ordered version migrations. Re-executing idempotent `CREATE IF NOT EXISTS` can remain a recovery tool, not the normal open path.

## Application services

### CaptureService

Input:

```text
StablePasteboardCapture {
  source_generation,
  observed_at,
  observed_frontmost_app,
  optional_content_origin,
  normalized_snapshot
}
CaptureMode { Watch, Manual, SetupSeed? }
```

Responsibilities in one clear sequence:

1. Read policy snapshot.
2. Evaluate pause/ignored app/sensitive filter according to explicit mode matrix.
3. Check restore suppression using operation/generation.
4. Insert/reuse source snapshot and event in one transaction.
5. Enqueue derived jobs in same transaction.
6. Update relevant revisions in same transaction.
7. Return typed outcome (`Stored`, `DeduplicatedEvent`, `SuppressedRestore`, `SkippedPolicy`, `TransientCapture`).
8. Schedule retention asynchronously or explicitly after commit; do not hide a large purge in the core capture transaction.

### RetrievalService

- Builds one query plan from query + filters + cursor.
- Executes one deduplicated snapshot candidate stream.
- Emits structured match evidence: fields, snippets, source, rank features.
- Paginates on the final total ordering.
- Loads canonical documents set-wise.
- `RecallService` consumes evidence/rank and applies tested selection policy; confidence is separately defined.

### SnapshotService

Distinct methods/types:

- `get_snapshot_metadata(id, event_limit)` — no BLOB.
- `get_snapshot_manifest(id)` — representation metadata, no BLOB.
- `read_representation(id,item,uti)` — one payload stream/bytes.
- `prepare_restore(id)` — intentionally loads all source payloads and validates them.
- `export_representation_atomic(...)` — targeted read + atomic destination replacement.

### RestoreService

1. Load/validate full restore plan.
2. Prepare all native pasteboard objects before mutation.
3. Snapshot current pasteboard for best-effort rollback.
4. Write restored objects.
5. Record suppression token with operation ID and resulting pasteboard generation/hash.
6. If write fails, attempt rollback and return structured outcome.

The exact order of marker/write may need a small platform experiment because the final pasteboard change count is only known after write. The design must ensure the watcher cannot observe a restored generation before a matching suppression record is visible; a coordinator process makes this easiest, but a short transaction/protocol can also solve it.

## Native app transport

### Near term

Keep subprocess calls, but:

- use read-only/current DB opens;
- batch startup status/settings/recent where useful;
- add metadata/preview commands;
- make cancellations terminate child work;
- retain stale UI data on refresh failures.

### Long-term gate

After profiling, a long-lived `clipmem service api`/`clipmemd` may own:

- the write connection and job claims;
- watcher lifecycle;
- Unix-domain-socket JSON-RPC (or another simple local framed protocol);
- revision/event subscription;
- payload streaming to the app;
- request cancellation.

CLI commands can attempt the service and fall back to direct read-only DB access where safe. Mutating commands should not silently use two different writers without a clear ownership rule.

Do not implement this until P0 DB/read corrections are measured. If subprocess latency becomes acceptable, keep the simpler architecture.

## Revision/event contract

Define categories and mutation matrix:

- `archive_content`: source snapshots/events/source metadata changes.
- `projection`: canonical document/index changes.
- `ocr`: OCR result changes.
- `storage`: physical size/derivative/storage state changes.
- `settings`, `service`, `app_preferences`.

Each committed application transaction emits one revision record with operation ID and affected snapshot IDs where practical. The native app can subscribe to events or poll one cheap read-only row. Per-snapshot document/content revisions make cache invalidation precise.

## Migration strategy

1. Add new tables/columns alongside old caches.
2. Backfill canonical documents in bounded batches using the one builder.
3. Dual-write old and new projections temporarily from application mutation code.
4. Run shadow comparisons on reads/tests; record mismatches.
5. Switch retrieval/detail to new document.
6. Remove broad legacy triggers/caches only after invariant and rollback window.
7. Preserve source tables throughout.

For source-preserving image migration, do not attempt to reconstruct original bytes from already optimized WebP rows unless the archive retained them; mark those rows as legacy-transcoded and preserve current bytes as the source going forward. Be honest in migration reporting.

## Deliberate non-goals

- Security hardening.
- Cloud sync or multi-user remote service.
- Semantic/vector search.
- Replacing SQLite.
- A generic event-sourcing framework.
- A UI rewrite.
