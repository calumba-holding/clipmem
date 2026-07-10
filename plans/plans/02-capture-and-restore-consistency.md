# Plan 2 — Capture and restore consistency

**Priority:** P0  
**Primary owners:** macOS platform adapter, capture store, watcher/setup/CLI orchestration  
**Depends on:** none; coordinate revision/document updates with plans 1 and 4

## Problem and evidence

- Watch, capture-once, and setup seed apply different policy and side effects (`src/cli/commands/runtime.rs:32-159`, `src/cli/commands/runtime.rs:206-252`, `src/cli/service/manage.rs:236-249`).
- Pasteboard capture has no end-generation check (`src/platform/macos.rs:26-60`).
- Content-origin inference overwrites actual frontmost app (`src/platform/macos.rs:126-150`).
- Restore suppression marker is consumed by one contender (`src/db/store/capture.rs:132-190`; `src/db/schema.sql:314-324`).
- Restore clears before construction/validation (`src/platform/macos.rs:63-92`).

## Required outcome

All capture entry points call one application service with an explicit mode/policy contract. A stored snapshot comes from one stable pasteboard generation. Restore cannot destroy the prior clipboard on preparation failure and cannot be re-captured by concurrent legitimate watchers.

## Scope

- Stable pasteboard read protocol.
- Observed app vs inferred origin model.
- Common capture policy/service and typed outcomes.
- Restore preparation/write/rollback protocol.
- Generation-aware suppression protocol.
- OCR enqueue/revision/notification/retention handoff consistency.
- Setup seed behavior decision.

Out of scope: security policy changes; cross-device clipboard; UI redesign.

## Design decisions

### 1. Stable platform capture

Introduce a platform result:

```text
StableCapture {
  before_change_count,
  after_change_count,       // equal on success
  observed_at,
  observed_frontmost_app,
  content_origin,
  items/raw representations
}
```

Algorithm:

1. Read `changeCount` (`before`).
2. Read frontmost application immediately adjacent to payload capture.
3. Enumerate all items/types and copy data.
4. Read `changeCount` (`after`).
5. If `before == after`, normalize/build snapshot.
6. Otherwise discard all captured bytes and retry up to 3 attempts with a short bounded delay (for example 2–10 ms); never merge attempts.
7. If unstable after attempts, return `TransientPasteboardChanged` and let watch poll again. Manual capture reports a clear transient error/outcome.

Do not store partial data. Do not use only item count/type equality as the generation check.

### 2. Preserve actual and inferred app identity

Capture event fields:

- `frontmost_app_name`, `frontmost_app_bundle_id`: actual observed app only.
- `content_origin_name`, `content_origin_bundle_id`, `content_origin_kind`: optional inference from Chromium metadata.

Ignore policy uses actual observed bundle ID. Search/UI can expose “Copied while in X” and optional “Content originated from Y.” Migrate existing rows with origin null; do not relabel history heuristically.

### 3. One capture application service

Create a service with explicit dependencies (`Database`/transaction, clock, notifier/job scheduler) and input:

```text
CaptureMode = Watch | Manual | SetupSeed
CaptureRequest { stable_capture, mode }
CaptureOutcome =
  Stored { snapshot_id, event_id, new_snapshot }
  SuppressedRestore { operation_id }
  SkippedPaused
  SkippedIgnoredApp { bundle_id }
  SkippedSensitive
  Duplicate/ObservedExisting { snapshot_id, event_id }
  TransientPlatformChange
```

Policy matrix (recommended):

| Rule/side effect | Watch | Manual (`capture-once`) | Setup seed |
|---|---:|---:|---:|
| pause | enforce | enforce by default; `--override-pause` only if explicitly added | enforce |
| ignored app | enforce | enforce by default; explicit override only | enforce |
| sensitive/API-key filter | enforce | enforce | enforce |
| restore suppression | enforce | enforce | enforce |
| enqueue OCR when enabled | yes | yes | yes if seed retained |
| revision + notification | yes | yes | yes |
| retention scheduling | yes | yes | once after setup |

Prefer removing setup seed entirely. If retained, expose `--seed-current-clipboard` (default false for new installs) or at minimum state the mutation before performing it and route through this service.

The core store transaction should:

1. read/check suppression state and policy snapshot that must be transaction-consistent;
2. insert/reuse snapshot and event;
3. enqueue derived jobs;
4. update revisions;
5. commit.

Notification and expensive retention execute after commit. Capture outcome indicates whether follow-up is needed.

### 4. Restore operation protocol

Add `restore_operations` (or evolve `pending_restores`) with:

- operation ID;
- snapshot ID/fingerprint;
- state `preparing|written|expired|failed`;
- expected/result pasteboard change count when known;
- created/expires timestamps;
- optional writer instance ID.

Protocol:

1. Load all source payloads intentionally and validate supported restore plan.
2. In platform code, construct every `NSPasteboardItem` and set every representation before touching general pasteboard. A failure here returns without mutation.
3. Capture a best-effort rollback plan from current pasteboard using a bounded payload policy. At minimum preserve the same representations the app normally captures; if rollback capture fails, continue only with an explicit result field that rollback is unavailable.
4. Create restore operation in DB as `preparing` with target fingerprint and short expiry.
5. Write prepared items to pasteboard; obtain resulting `changeCount`.
6. Mark operation `written` with resulting generation in DB immediately.
7. Watchers check target hash + generation against all nonexpired `written` operations. The operation is **not deleted by the first watcher**. It remains suppressing that exact generation until the pasteboard advances or expiry.
8. A cleanup pass expires old operations.
9. On write failure after clear, attempt rollback; mark failed and return `{ write_error, rollback_attempted, rollback_succeeded }`.

There is a race between pasteboard write and recording resulting generation. Resolve it explicitly:

- Preferred without daemon: watcher, when seeing a new generation, waits a very short bounded grace period/rechecks operations before storing; restore writes `preparing` beforehand with target fingerprint, allowing hash-based suppression during the gap, then pins generation.
- With future coordinator: restore and watch are serialized in one process, eliminating the gap.

The implementation must include a deterministic concurrency test at the DB service layer even if AppKit is mocked.

### 5. Remove/replace the one-shot trigger

Once all capture writes go through the service, remove `capture_events_restore_suppression_bi`. During transition:

- change it so it checks a nonconsuming operation, or
- disable direct capture-event inserts outside tested migration/fixtures.

Do not keep both a consuming Rust path and consuming trigger.

## Implementation sequence

1. Write behavior tests for the policy matrix and two-watcher restore race.
2. Introduce platform traits/fakes for pasteboard reader/writer and frontmost app so stable-read/rollback logic is testable outside AppKit.
3. Implement stable capture loop and separate app-origin fields in model.
4. Add schema migration for content origin and restore operations.
5. Implement `CaptureApplicationService` and move policy/store/job/revision sequencing into it.
6. Change watch, capture-once, and setup to call the service; delete duplicated sequences.
7. Decide/remove explicit setup seed; update outputs/docs.
8. Implement restore-plan preparation and rollback-capable writer.
9. Implement operation lifecycle and nonconsuming generation suppression; transition/remove old marker trigger.
10. Ensure notifier posts only after committed revision and only once per operation.
11. Add cleanup/expiry to ordinary write/service maintenance without making reads mutate.
12. Update CLI JSON/text outcomes so skipped reasons are explicit and stable.

## Edge cases and failure modes

- Empty pasteboard is a valid stable snapshot if current product contract stores it; policy must be consistent.
- App changes while pasteboard stays stable: record app sampled during stable attempt; do not retry solely for app change.
- Representation disappears/throws during read: discard attempt and retry only if generation changed; otherwise return platform error with UTI/item context.
- Two restore operations with same fingerprint but different generations both suppress only their own generations.
- User manually copies identical bytes after restore at a new generation: do not suppress merely by hash once the restore generation has passed.
- Watcher starts after restore but before expiry: it should suppress the recorded restored generation.
- DB unavailable after pasteboard write: return explicit “clipboard restored, suppression registration incomplete” and let future coordinator reduce this case; do not claim full success silently.
- Rollback itself can fail; never hide it.

## Tests

- Stable read: generation changes after item 1; first attempt discarded, second captured wholly.
- Stable read exhausts retries; no store call.
- Every mode × pause/ignore/sensitive/OCR matrix.
- Actual ChatGPT/Codex app retained while Chromium origin is separate; ignore uses actual bundle.
- Two concurrent capture service calls for one restore generation both return suppressed and insert no event.
- Identical manual copy at next generation stores normally.
- Restore item construction failure leaves fake current pasteboard untouched.
- Write failure invokes rollback and reports result.
- Setup behavior explicit and documented.
- Migration from old `pending_restores` state safely expires legacy rows.

## Acceptance criteria

- No command directly assembles capture policy/store/OCR/revision sequences outside `CaptureApplicationService`.
- A snapshot is stored only from equal before/after pasteboard generation.
- Pause and ignored-app semantics match docs for every capture entry point.
- Actual frontmost app is never replaced by inferred origin.
- Restore preparation errors leave current pasteboard unchanged.
- Concurrent watchers cannot store the restored generation.
- Notifications correspond to committed revision changes.

## Rollout and rollback

Ship schema additions first while retaining old columns. During one transition release, write both old marker (if needed for old binaries) and new operation state, but new capture logic must prefer new operations. A rollback binary may ignore new fields. Remove the old trigger/table only after service compatibility policy permits it.
