# Plan 6 — Source-preserving image storage and previews

**Priority:** P1  
**Primary owners:** archive model/storage, image worker, export/restore/detail APIs, Swift preview  
**Depends on:** plans 3–5  
**Replaces:** in-place logical image rewrite behavior

## Problem and evidence

Current optimization replaces UTI, bytes, hash, compression metadata, and snapshot fingerprint in place (`src/db/store/optimize.rs:532-664`). It decodes to RGBA and tests pixel equality, which does not preserve exact container bytes/metadata. This conflicts with exact rich restore/raw export and destabilizes content identity.

## Required outcome

Maintenance never changes the logical source representation or source fingerprint. Fast previews and optional disk savings use versioned derivatives or a reversible physical encoding. Export/restore produce exact logical source bytes.

## Scope

- Immutable source rule and schema constraints/application invariants.
- Derivative table and image preview API.
- Migration of current optimizer metadata/state.
- Durable derivative jobs (plan 5).
- Optional reversible physical encoding design gate.
- UI/storage command semantics and metrics.

Out of scope: lossy user-authorized transcoding as a general editor; security.

## Product contract

Define three distinct operations:

1. **Generate previews/derivatives** — source-safe, default, may consume additional space but improves UX.
2. **Compact database** — SQLite physical maintenance; does not alter logical payload.
3. **Re-encode source destructively** — not part of transparent optimization. If ever offered, it requires explicit user consent, a new snapshot/source identity, and clear loss semantics.

Rename user-facing commands accordingly. Do not call derivative generation “compress stored originals.”

## Data model

Add `representation_derivatives` as described in target architecture, with source key and algorithm version. Include:

- source snapshot/item/UTI/raw SHA;
- derivative kind (`preview`, `thumbnail`, optionally `reversible_storage`);
- output UTI/codec and bytes/locator;
- dimensions and decoder metadata;
- status/reason;
- encoder version/options hash;
- created/verified timestamps;
- unique dedupe key.

Source `item_representations` fields remain:

- original UTI;
- exact original byte length/hash/blob;
- source identity metadata.

Deprecate `image_compression_*` columns as source mutation status. Preserve them temporarily for legacy audit/migration.

## Preview derivative design

- Decode supported still images with bounds checked before large allocation where possible.
- Respect orientation/color profile for rendered preview. The derivative need not preserve source metadata, but visual output should be correct.
- Cap long edge/pixel count for UI preview; choose a lossless or high-quality codec based on measured decode support. Since source remains, derivative can be optimized for preview.
- Store one shared derivative by source raw SHA when bytes are identical across snapshots, with a mapping/reference model that preserves cleanup.
- Swift requests a preview by source/version key and receives bytes or a managed temp-file/stream without running full `get`/`export`.
- Cache key includes source hash, derivative kind, encoder version.

## Existing optimized archives

There is no evidence original bytes are retained after current in-place WebP rewrite. Migration must not pretend otherwise.

For rows marked compressed:

- treat current WebP bytes/UTI/hash as the immutable source going forward;
- add provenance `legacy_transcoded_by_clipmem` and, where present, original byte length/hash metadata as historical claims only;
- do not set a flag implying original bytes are reconstructable;
- source fingerprint remains whatever current archive uses at migration time;
- generate derivatives from current source normally.

Rows marked skipped/uncompressed keep current source and migrate status into job/derivative eligibility.

## Optional reversible physical encoding

Treat as a separate subproject/gate. A valid implementation stores compressed physical bytes but reconstructs exact original bytes before any logical consumer/fingerprint. Requirements:

- algorithm/format supports arbitrary source byte stream, not image pixels (general compression such as zstd would preserve bytes; adding a new dependency requires evaluation);
- store original length/hash and verify on decode;
- atomic migration with source retained until verified;
- lazy/eager decode API with bounded memory;
- corruption behavior never substitutes derivative bytes for source;
- export/restore byte-for-byte fixtures;
- rollback can restore plain source storage;
- measured savings account for already-compressed formats and CPU cost.

This may be less beneficial than expected for PNG/JPEG/HEIC/WebP. Do not implement without corpus measurements.

## Commands/API

- `storage build-previews [--limit --operation ...]`
- `storage preview-status`
- `storage compact`
- Existing `storage optimize-images` becomes deprecated alias with explicit source-safe behavior, or remains named `legacy-transcode-images` only for backward compatibility and is hidden/disabled by default.
- Detail API returns derivative availability/version in metadata.
- `preview` command/service endpoint reads exactly one derivative or can generate/enqueue and report pending.
- Export/restore always target source representation.

## Implementation sequence

1. Add exact-source invariant tests around optimizer: source UTI/bytes/hash/fingerprint must remain unchanged.
2. Add derivative schema/provenance migration; classify legacy compressed rows honestly.
3. Implement source-keyed derivative repository and cleanup/refcount query.
4. Add job kind/encoder using plan 5 claims and bounded memory.
5. Add targeted preview read API from plan 3 and include derivative metadata in snapshot manifest/document.
6. Convert Swift preview to the new API/cache key; remove get→export temp flow where transport permits. If temp files remain, they are created directly from derivative payload and managed atomically.
7. Change storage UI/CLI wording, progress, revisions, and docs.
8. Disable current in-place replacement for new operations. Keep code only for legacy migration tests until deletion.
9. Measure derivative size/decode latency and source archive corpus compressibility.
10. Decide reversible physical encoding gate separately; do not block preview shipping.
11. Remove/deprecate source compression columns in a later schema migration.

## Edge cases

- Animated GIF/WebP: preview may use first frame with explicit metadata; source remains exact.
- HEIC/TIFF/color profiles/orientation: rendered preview fixture must visually orient correctly.
- Huge/corrupt/decompression-bomb-like dimensions: skip with reason before allocation where possible; security framing is out of scope, but resource correctness is in scope.
- Derivative missing/corrupt: delete/requeue; never affect source restore/export.
- Snapshot deleted while shared derivative remains referenced elsewhere: cleanup only when no source mapping.
- Source duplicated across snapshots: derivative reused.
- Encoder version changes: old derivative remains usable until new ready; active-version policy explicit.
- Disk full while writing derivative: source untouched, job retry/failure truthful.

## Tests

- Byte-for-byte source UTI/blob/hash/fingerprint before/after preview generation.
- Restore/export exact equality after derivatives and compaction.
- Legacy compressed-row migration provenance.
- Pixel/orientation/color-profile visual metadata fixtures where deterministic testing permits.
- Shared-source derivative dedupe/cleanup.
- Corrupt derivative recovery without source impact.
- Large image memory budget and cancellation.
- Swift preview cache invalidates on source/derivative version and does not load full source snapshot.
- Optional reversible encoding round trips arbitrary image file bytes exactly, including metadata chunks.

## Acceptance criteria

- No source representation update occurs in normal image maintenance.
- Snapshot source fingerprint is stable across preview/compact work.
- Detail preview requires one targeted derivative read and no all-snapshot BLOB hydration.
- Existing optimized rows are labeled as legacy-transcoded without false recovery claims.
- Storage UI/docs clearly distinguish source, derivative, and physical compaction.
- Any disk-saving reversible encoding proves exact bytes and is separately gated.

## Rollout and rollback

Add derivatives while keeping current sources. Ship preview path first. Disable in-place optimizer through command behavior/feature flag but retain read compatibility with its columns. Rollback simply returns UI to source export preview; source is intact. Do not drop legacy metadata until archives have migrated and documentation no longer relies on it.
