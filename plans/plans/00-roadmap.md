---
status: accepted
created: 2026-07-10
last-verified: 2026-07-11
owners: []
---

# Implementation roadmap

## Delivery rules

- No plan authorizes a big-bang rewrite.
- Source tables and the CLI contract remain available throughout migration.
- Each schema change must have forward migration, fixture-based upgrade test, invariant validation, and a rollback/compatibility story.
- New application services are plain, explicit orchestration boundaries; do not introduce abstractions without at least two real call sites or a clear invariant owner.
- Security work remains out of scope.
- Save benchmark and query-plan evidence before and after performance changes.

## Dependency graph

```text
1 Retrieval correctness ───────────────┐
                                      ├─> 4 Canonical projections
2 Capture/restore consistency ────────┤          │
                                      │          ├─> 5 Durable jobs
3 DB open + targeted reads ───────────┘          ├─> 6 Source-preserving images
                                                 └─> 7 Service boundary gate

3 + 6 + 7 decisions ───────────────────────> 8 Native app resilience/UX
4 ─────────────────────────────────────────> 9 Text/content contracts
all plans ─────────────────────────────────> 10 Verification/hygiene
```

Plans 1–3 can be developed in parallel if shared schema/API decisions are coordinated. Plan 4 must reuse the unified search document introduced by plan 1 rather than creating a second competing projection.

## Releases

### Release A — correctness stop-the-bleeding (P0)

- Plan 1: truthful score/confidence; one deduplicated search stream; correct text flags.
- Plan 2: common capture policy; stable capture; safe restore/suppression.
- Plan 3: current-schema open modes; metadata-only reads; atomic export.

Gate: all old CLI JSON fields remain parseable; migration tests pass from every supported fixture; benchmark quality does not regress; no BLOB is read on list/status/revision paths.

### Release B — derived-state and worker foundation (P1)

- Plan 4: composite FK, canonical versioned document, controlled dual-write/backfill.
- Plan 5: durable job claims/leases.

Gate: shadow comparison reports zero unexplained projection mismatches; two-worker tests are deterministic; old triggers can still be re-enabled for rollback.

### Release C — source-safe storage and app transport decision (P1/P2)

- Plan 6: immutable source image semantics and derivatives/reversible storage.
- Profile and decide plan 7.
- Plan 8 consumes targeted reads/events whether transport remains subprocess or becomes a service.

Gate: exact source bytes/fingerprint unchanged by optimization; app preview latency and process counts meet explicit targets.

### Release D — fidelity and hardening (P2)

- Plan 9: versioned HTML/RTF/content roles.
- Plan 10: complete invariants, docs, plan lifecycle, CI benchmark artifacts.

## Decision gates

### Long-lived service gate

Proceed with plan 7 only if, after plans 1–3:

- median warm `service revision` subprocess request remains above 25 ms or causes visible energy/process churn;
- history/detail P95 remains above the agreed UI target despite targeted reads;
- write contention or job ownership is materially simpler with one coordinator;
- launch/lifecycle complexity is acceptable on supported macOS versions.

Otherwise retain the simpler direct CLI/subprocess architecture and implement only batching/event improvements.

### Reversible physical image compression gate

Do not evict source BLOBs until:

- byte-for-byte reconstruction is proven across all supported image fixtures;
- restore/export/fingerprint operate on logical original bytes;
- crash recovery and migration rollback are tested;
- measured disk savings justify complexity.

Preview derivatives can ship earlier because they do not alter source.

## Cross-plan contracts that must not drift

- `CaptureOutcome` and capture mode policy matrix.
- `SnapshotMetadata`/`RepresentationManifest`/`RepresentationPayload` cost boundaries.
- Canonical `SnapshotDocument` builder/version.
- Search final ordering and cursor version.
- Job state machine/lease protocol.
- Source fingerprint definition.
- Revision mutation matrix.
- Native app cache key includes content/document version.
