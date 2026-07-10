# Plan 8 — Native app resilience and UX semantics

**Priority:** P2 (some pieces may accompany P0 APIs)  
**Primary owners:** Swift client, AppModel, HistoryModel, detail views  
**Depends on:** plan 3; preview portions depend on plan 6; transport portions follow plan 7 decision

## Problem and evidence

- Recent rows are cleared on transient refresh error (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:132-142`).
- External history refresh replaces loaded pages and may retain stale selected detail (`HistoryModel.swift:82-125`).
- Generation checks ignore stale responses but do not cancel underlying history subprocesses.
- Command timeout is indistinguishable from cancellation and uses TERM only (`CommandRunner.swift:28-91`).
- Preview task key is snapshot ID, while content can change under that ID (`SnapshotDetailView.swift:56-59`).
- Self-ignore marker is global, not archive-specific (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:542-550`).
- “Copy” can flatten rich content while “Restore” preserves formats (`SnapshotDetailView.swift:166-219`).

## Required outcome

The app retains useful stale state through transient failures, cancels real work, refreshes precisely by content/revision, uses targeted previews, scopes configuration by archive identity, and labels clipboard actions according to fidelity.

## State model decisions

Represent asynchronous surfaces with explicit state rather than nil/empty overloads:

```text
LoadState<Value> = idle | loading(previous?) | loaded(value, refreshedAt) | failed(error, previous?)
```

At minimum apply to service status, recent preview, history page, detail, and image preview. Preserve `previous` on recoverable errors.

Errors have categories:

- timeout;
- user cancellation;
- service unavailable/reconnecting;
- migration/setup required;
- not found/stale selection;
- command/platform failure.

UI behavior differs by category.

## AppModel refresh orchestration

- Replace three independent startup subprocesses with a batch endpoint if plan 7/service supports it, or sequence/limit concurrency after plan 3 measurements.
- `refreshRecentPreview` retains previous rows on failure and records nonblocking refresh error.
- Archive revision events/poll changes are coalesced by category and configuration generation.
- Do not set global `lastError` for every transient two-second poll failure; use connection/refresh status and backoff.
- Revision/category mutation matrix from plan 4 determines which surfaces refresh.
- A DB override change resets archive-scoped state, installs self-ignore idempotently in the new archive, and prevents old-generation responses applying.

## HistoryModel

- Retain `pageTask` and `detailTask`; cancel before new query/filter/mode/selection load.
- Transport cancellation must terminate the CLI process or send service cancel.
- Preserve loaded window on external changes when possible:
  - if affected snapshot IDs/event kinds are known, update/remove/reload targeted rows;
  - otherwise reload first N pages equal to currently loaded row count up to a cap, preserving scroll/selection;
  - if only OCR/projection of selected snapshot changed, reload detail and row without collapsing pages.
- Track selected row by event ID for timeline and snapshot ID for snapshot modes explicitly; avoid ambiguous fallback where duplicate event rows share snapshot.
- Detail cache key includes snapshot content/document revision.
- If selected snapshot is forgotten, choose nearest adjacent row, not always first, where UX permits.

## CommandRunner/subprocess transport

If subprocess remains:

- Add `CommandTimeoutError` with command category and deadline.
- On cancellation/timeout: send TERM, wait bounded grace (e.g. 500 ms), then KILL; ensure process group/descendants are handled where commands can spawn children.
- Never block indefinitely in `waitUntilExit` after cancellation; use asynchronous termination observation.
- Close pipes exactly once and ensure reader semaphores cannot be double-signaled into inconsistent state.
- Impose sensible timeouts for all UI operations: short reads, longer maintenance, no arbitrary timeout for user-visible streaming operation without cancellation.
- Log request ID/duration/exit category, not payload content.

If service transport ships, preserve the same client protocol and error types.

## Preview flow

- Detail metadata includes preview descriptor: source/derivative ID, item, UTI, content hash/version, availability/status.
- Task/cache ID is full descriptor, not snapshot ID.
- Request targeted derivative/payload; do not call full export.
- Cancel previous preview request on selection/version change.
- Manage temp files through a dedicated preview cache with bounded size/lifetime, or decode streamed bytes directly if safe.
- On derivative pending, show stable placeholder and optionally enqueue; on failure, allow retry without affecting source.

## Clipboard action semantics

Use explicit labels:

- `Copy text` — writes flattened/extracted plain text.
- `Copy original` — writes exact saved snapshot formats (same mechanism as restore, but wording can distinguish replacing current clipboard from historical restore).
- `Restore original` may remain in history context if product values that verb.
- For image-only: `Copy image` uses exact source snapshot.

Never make a generic “Copy” choose lossy/plain behavior merely because text exists. Keep keyboard shortcuts/help aligned.

## Self-ignore and preferences

- Add archive instance ID from plan 3 to app state.
- Self-ignore installation is an idempotent settings add on each active archive; no boolean is necessary, or cache by archive instance ID.
- Preference revision notification must not create DBs (Rust plan/findings); app handles invalid path as configuration error while retaining old archive until user confirms, if product design allows.

## Implementation sequence

1. Add Swift tests for stale-while-revalidate, task cancellation, detail version invalidation, and archive-scoped self-ignore.
2. Introduce transport/client protocol so view models can be tested independent of subprocess/service.
3. Add explicit load/error states incrementally, starting with recent/history/detail.
4. Add task ownership/cancellation to HistoryModel and AppModel refresh coordinator.
5. Harden CommandRunner or service cancellation/error mapping.
6. Consume metadata-only/detail and preview APIs from plans 3/6.
7. Implement precise revision/event refresh and page preservation.
8. Change copy action labels/behavior and accessibility/help text.
9. Scope self-ignore/preferences by archive instance.
10. Add UI instrumentation tests/benchmarks and update docs/screenshots where needed.

## Edge cases

- Refresh fails while no prior data: show empty/error state; with prior data, retain and mark stale.
- Selected timeline event disappears but snapshot remains: choose appropriate row/detail without silently changing event metadata.
- Query changes rapidly: only newest task may mutate state; old process is actually terminated.
- App closes during maintenance/preview: cancel request/cleanup temp file while durable operation semantics remain correct.
- DB override changes mid-request: generation/archive ID rejects old response.
- Preview descriptor changes under same snapshot ID: old temp is removed after new request safely.
- Service event gap: full revision refresh without repeated banners/data clearing.

## Tests

- AppModel recent failure preserves previous rows and sets stale error.
- Concurrent refresh coalescing and configuration generation.
- History cancellation fixture asserts process/service cancel invoked.
- External content revision reloads selected detail; storage-only revision does not unnecessarily reload text.
- Loaded-page preservation across external change.
- Timeout vs cancellation UI mapping; TERM-ignoring child is killed.
- Preview cache key/version, cancellation, temp cleanup.
- Copy text vs copy original pasteboard fixture behavior.
- Self-ignore on two archive instance IDs.

## Acceptance criteria

- Transient read/poll failures do not present an empty archive when stale data exists.
- Obsolete history/detail work is cancelled at transport/process level.
- Same-ID content/document changes invalidate row/detail/preview correctly.
- Image preview uses one targeted read.
- Clipboard action labels accurately state fidelity.
- Self-ignore is present in every active archive and DB override changes do not leak old responses.
- Existing accessibility/Markdown/link behavior remains intact.

## Rollback

View-model/client protocol changes are internal. Keep old subprocess transport as fallback if service path is involved. Copy labels can be feature-reverted without schema impact. Targeted preview API fallback may use source payload directly, but must not restore the all-snapshot `get`→`export` amplification.
