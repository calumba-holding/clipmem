---
status: implemented
created: 2026-07-10
last-verified: 2026-07-11
implemented-in: schema v23, builder v3 merged implementation
owners: []
---

# Plan 4 — Schema integrity and canonical projections

**Priority:** P1  
**Primary owners:** schema/migrations, model projection builder, database store/read layers  
**Depends on:** plans 1 and 3 foundations  
**Enables:** simpler triggers, consistent detail/search/filter semantics, reliable rebuilds

## Problem and evidence

- Representations lack a composite FK to items and orphan rows are silently ignored (`src/db/schema.sql:25-41`; `src/db/read/snapshot.rs:280-357`).
- Snapshot meaning is built independently in live builders, migrations, detail flattening, optimizer rebuilds, SQL triggers, and caches.
- Trigger order can temporarily drop fields from literal haystack (`src/db/schema.sql:232-286`).
- Historical app, native text, OCR, URLs, flags, and display preview have inconsistent owners.

## Required outcome

Relational integrity is enforced. One versioned builder creates the canonical snapshot document used by search, list/detail projections, capability filters, and rebuild/migration. Derived state can be verified and rebuilt without reading implementation-specific trigger order.

## Scope

- Composite FK and data repair migration.
- Canonical document schema and builder (extend plan 1 search document; do not duplicate).
- Structured projection/content roles.
- Explicit transactional rebuild ownership.
- Dual-write/shadow-read migration.
- Reduction/removal of broad legacy triggers/caches.
- Invariant checker/doctor integration.

Out of scope: parser quality improvements (plan 9), source image redesign (plan 6), security.

## Canonical document contract

Extend `snapshot_search_documents` from plan 1 into `snapshot_documents` (rename only through a migration/view that avoids churn) with:

- source: `snapshot_id`, `source_fingerprint`, `builder_version`;
- event aggregate: first/last observed, capture count, last observed app, distinct historical apps/origins;
- display: preview, summary, primary kind/item count/total bytes;
- text: best native text + UTI, structured fragments, HTML text, RTF text, OCR text/status;
- links: distinct normalized URLs and file paths;
- capabilities: factual booleans for native text/OCR/URL/file/image/PDF/binary;
- content/document revision and updated time.

Structured arrays can be normalized child tables or JSON. Choose based on query needs:

- fields used in SQL filters/indexes should be columns/child rows;
- detail-only fragments may be JSON with schema version, provided deterministic serialization and migration tests exist.

Display preview is never reused as a factual capability.

## One builder

Create `SnapshotDocumentBuilder` in the model/application domain. Inputs are explicit adapters:

- representation manifests and only the bytes needed to derive native text (during initial source insertion/rebuild);
- capture event aggregate;
- OCR aggregate;
- projection builder version/config.

Outputs one pure `SnapshotDocumentDraft`. The same code is used for:

- new capture;
- migration/backfill;
- OCR completion/clear;
- event insertion/deletion/update;
- representation source changes/legacy repairs;
- doctor rebuild.

Do not copy classification/parsing logic into SQL migration code. Migration code invokes the Rust builder in bounded batches. SQL triggers may enqueue/mark a document dirty, but do not reconstruct the full document with duplicated concatenation.

## Integrity migration

### Preflight

Before table rebuild, query:

- representations without matching item;
- duplicate/invalid item indexes;
- item byte totals vs representation sums;
- snapshot item/byte totals vs items;
- missing source snapshot;
- malformed image compression metadata.

Policy:

- if an orphan representation can be unambiguously associated only by creating a missing item, do **not** invent primary kind/preview silently;
- default to abort migration with a report and `doctor repair` option that quarantines/deletes confirmed orphans after user-visible dry run;
- tests may choose deletion for impossible rows, but production migration must be explicit.

Rebuild `item_representations` with:

```sql
FOREIGN KEY (snapshot_id, item_index)
  REFERENCES snapshot_items(snapshot_id, item_index)
  ON DELETE CASCADE
```

Ensure parent uniqueness exists and foreign keys are enabled during migration/verification.

## Mutation ownership

Create one transaction helper/service per source mutation that:

1. changes authoritative rows;
2. rebuilds/updates document from canonical builder or marks it dirty + synchronously updates fields required for immediate reads;
3. updates FTS/index rows through narrow document-table triggers or explicit statements;
4. bumps revision categories;
5. commits.

Prefer narrow triggers only for mechanical index mirroring from `snapshot_documents` to FTS. Remove triggers that separately recreate business projections from capture/representation events once all writes use services.

Direct SQL test fixtures must either invoke rebuild explicitly or be clearly marked low-level corruption fixtures.

## Migration/rollout sequence

1. Add invariant tests and a read-only `doctor --verify-invariants` report before changing schema.
2. Add composite FK migration preflight/repair flow.
3. Extend plan 1 document schema and implement pure versioned builder.
4. Backfill documents in batches ordered by snapshot ID; persist progress and builder version. Avoid one huge migration transaction for large archives.
5. Dual-write legacy caches and canonical document for all mutations.
6. Shadow compare:
   - best text/UTI;
   - URLs/file paths;
   - flags;
   - event aggregates/apps;
   - OCR status/text;
   - preview/summary where exact equality is intended.
7. Switch get/list/filter/stats/search consumers to canonical document incrementally.
8. Run invariant checker after every mutation in targeted tests and on fixture migrations.
9. Stop writing legacy caches; retain read-only compatibility for one release.
10. Drop broad legacy triggers/caches/FTS in a later schema version after rollback window.
11. Update architecture docs with authoritative/derived ownership diagram.

## Failure modes and edge cases

- Very large archives: backfill must resume and expose progress; current reads can use legacy rows until each snapshot/document is ready.
- Source row changes during backfill: compare source fingerprint/revision before commit; retry stale snapshot.
- OCR updates during backfill: builder reads latest aggregate in same write transaction or marks dirty afterward.
- Event deletion changes last app/history; document must recompute aggregate, not decrement strings blindly.
- Legacy optimized images have already changed source; treat current bytes as source, mark legacy provenance, do not claim original recovery.
- Unknown UTI remains binary/opaque but is represented in manifest and flags consistently.
- Builder version change queues/rebuilds documents; mixed versions are visible and search readiness policy is explicit.

## Tests

- Composite FK rejects orphan insert and cascades item deletion.
- Migration fixture with orphan aborts with exact report; repair dry run/execute behavior.
- Pure builder golden tests across plain text, URL, file URL, HTML, RTF, image+OCR, PDF, mixed items, unknown binary, historical apps.
- Live capture and migration builder produce byte-for-byte identical document serialization for same source.
- Every authoritative mutation updates document/FTS/revision in one transaction.
- Trigger-order tests become unnecessary because only narrow index mirroring remains; verify no stale document after rollback/failure.
- Backfill resume after injected crash and concurrent source update.
- Shadow comparison against legacy fixtures with documented intentional differences.

## Acceptance criteria

- `PRAGMA foreign_key_check` and custom invariants pass for all migrated fixtures.
- There is one production implementation of classification/text/link/capability document construction.
- Search, get/list, and filters consume the same factual document fields.
- Broad event/representation projection triggers are removed or no longer required for current writes.
- Derived state can be dropped/rebuilt from authoritative rows + OCR/settings with a documented command/test.
- Backfill is bounded, resumable, and reports readiness.

## Rollback

During dual-write, old caches/triggers remain available. A feature/schema readiness flag can switch reads back. Do not drop old derived tables in the same release that first switches reads. Composite FK is not rolled back; it corrects an invalid state and must be validated before migration commit.
