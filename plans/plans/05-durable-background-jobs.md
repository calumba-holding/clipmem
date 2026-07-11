---
status: implemented
created: 2026-07-10
last-verified: 2026-07-11
implemented-in: schema v22 merged implementation
owners: []
---

# Plan 5 — Durable background jobs

**Priority:** P1  
**Primary owners:** database store/job layer, OCR orchestration, image derivative/maintenance commands  
**Depends on:** plan 3 open modes; coordinate document/revisions with plan 4

## Problem and evidence

OCR candidates are selected without being claimed (`src/db/store/ocr.rs:28-128`). Image optimization similarly loads candidates without ownership (`src/db/store/optimize.rs:318-351`). Process-local guards do not prevent multiple watchers/app commands, and cancellation/crash leaves only implicit status behavior.

## Required outcome

Every expensive unit of background work is deduplicated, atomically claimed, leased, retried under a clear policy, completed idempotently, and observable as part of an operation. Multiple processes can work safely.

## Scope

- Job table/state machine and repository.
- OCR migration to jobs.
- Image derivative/optimization migration to jobs.
- Atomic claim/lease/heartbeat/completion.
- Operation-level progress/cancellation/resume.
- Revision/document integration.
- Multi-worker tests and bounded concurrency.

Out of scope: distributed remote workers, security, generic workflow DAGs.

## Data model

```text
job_operations
  id TEXT/UUID PRIMARY KEY
  kind TEXT
  state running|completed|completed_with_errors|cancel_requested|cancelled|failed
  requested_by, created_at, started_at, finished_at
  total_count, succeeded_count, skipped_count, failed_count
  options_json/version

jobs
  id INTEGER PRIMARY KEY
  operation_id FK nullable (a deduped job may be attached through join table if needed)
  kind TEXT
  dedupe_key TEXT
  algorithm_version INTEGER/TEXT
  state queued|leased|succeeded|failed|skipped|cancelled
  priority INTEGER
  not_before TEXT
  lease_owner TEXT
  lease_until TEXT
  attempts INTEGER
  max_attempts INTEGER
  last_error TEXT
  source_ref_json or normalized key columns
  result_ref_json
  created_at, updated_at, finished_at
  UNIQUE(kind,dedupe_key,algorithm_version)

operation_jobs(operation_id, job_id, state-at-attachment if needed)
```

Prefer normalized key columns for hot queries (OCR raw SHA; image snapshot/item/UTI/source hash) over opaque JSON alone.

## State machine

- enqueue is idempotent on `(kind,dedupe_key,algorithm_version)`;
- `queued -> leased` only by atomic claim;
- `leased -> succeeded|skipped|failed` only when `lease_owner` matches and lease is valid (allow completion just after expiry only under a documented compare-and-set rule; safer to reject stale completion);
- retryable failure becomes `queued` with incremented attempts and `not_before` backoff;
- terminal failure after max attempts remains `failed`;
- expired lease becomes claimable and increments a lease-loss metric/reason;
- cancel request prevents new claims for the operation; already leased job may finish or be cooperatively cancelled according to job kind;
- result-table updates and job completion occur in one transaction.

## Atomic claim

Use a short immediate transaction with SQLite-version-compatible SQL. Preferred shape:

1. Select IDs ordered by priority/not-before/created, including queued and expired leased jobs.
2. Update chosen IDs to leased with owner/until and increment attempts using a predicate that confirms they remain claimable.
3. `RETURNING` claimed source fields if supported by bundled SQLite; otherwise reselect by owner/token in same transaction.
4. Commit before OCR/decode work.

Claim small batches based on byte/memory budget, not 250 payload BLOBs. Payload is loaded after claim, one job at a time or bounded concurrency.

## OCR integration

- OCR dedupe key: source raw SHA + OCR algorithm/version/language configuration.
- Existing `ocr_results` may remain domain result table; job success writes/updates it, rebuilds affected documents, bumps OCR/projection revisions, and completes job atomically.
- New snapshots referencing an existing successful result do not enqueue duplicate work; they rebuild/link OCR document state.
- Failure classification: unsupported image → skipped terminal; transient Vision/platform/resource failure → retry; deterministic corrupt input → failed/skipped according to explicit taxonomy.
- `ocr run --retry-failed` creates/reattaches operations and resets only eligible terminal jobs with version-aware rules.

## Image integration

- Derivative job key: source raw SHA + derivative kind + encoder algorithm/version/options.
- Source-preserving plan 6 defines result table. Until plan 6 lands, do not run new in-place rewrite jobs through this system except as legacy maintenance explicitly named destructive.
- Candidate discovery inserts jobs without loading full BLOBs.
- Worker loads one source payload after claim, checks source hash still matches, encodes, writes derivative/result + document/storage revision, completes atomically.
- Memory budget uses source bytes plus estimated decoded pixels; reject/skip dimensions over policy before allocation where decoder metadata allows.

## Operation progress API

CLI streaming emits durable snapshots keyed by operation ID:

- discovered/total;
- queued/leased/succeeded/skipped/failed;
- logical bytes processed/saved where relevant;
- current phase;
- cancellation requested;
- final terminal summary.

If the client disconnects, work policy is explicit:

- foreground command default: request cancellation and stop claiming; claimed job may finish;
- service/background operation: continue; caller can inspect by ID.

Do not derive final report only from in-process counters.

## Implementation sequence

1. Define job kinds, terminal/retry taxonomy, state transition table, and operation semantics in code/docs.
2. Add schema/migration and indexes for claim query, operation progress, dedupe.
3. Implement repository with enqueue/claim/heartbeat/complete/fail/cancel and fake clock tests.
4. Add two-process/two-connection claim tests using a file-backed SQLite DB.
5. Migrate OCR enqueue/candidate/run paths; keep reading legacy pending statuses during transition, but one system owns claims.
6. Make OCR result/document/revision/job completion one transaction.
7. Implement operation progress and adapt CLI JSONL/final output additively.
8. Integrate image derivative work after plan 6 result schema is ready.
9. Add bounded worker pool and byte/pixel budget.
10. Remove process-local “single worker is enough” assumptions and old candidate selection APIs.
11. Add cleanup/retention for old terminal jobs/operations without deleting domain results.

## Edge cases

- Worker crashes after external OCR but before commit: lease expires and work repeats safely; completion is idempotent.
- Source representation deleted while queued/leased: terminal skipped `source_missing`; operation progresses.
- Source hash changes (legacy mutation): stale job skips and a new dedupe key is enqueued.
- Same OCR raw bytes referenced by many snapshots: one job/result, all affected documents update.
- Algorithm version bump: creates new jobs without overwriting old result until new succeeds; document chooses active version explicitly.
- Clock changes: use SQLite UTC timestamps consistently; leases tolerate modest wall-clock changes or use conservative durations.
- Long job needs heartbeat; heartbeat failure makes worker stop before committing stale result.
- Cancellation races completion: terminal completion wins only under explicit state predicate; operation summary remains consistent.

## Tests

- Two connections claim concurrently; no duplicate IDs.
- Lease expiry and reclaim; stale owner cannot complete.
- Retry backoff/max attempts/failure taxonomy.
- Idempotent enqueue and algorithm-version behavior.
- OCR shared raw SHA updates all snapshot documents once.
- Crash injection between claim, result write, document rebuild, revision, completion.
- Cancellation/disconnect behavior and resumable operation report.
- Bounded memory scheduling from candidate metadata.
- Query plan uses claim indexes at large seeded scale.

## Acceptance criteria

- No OCR/image worker processes a row it did not claim.
- Multi-worker tests never duplicate terminal result writes.
- Result + document/revision + job terminal transition are atomic.
- Operations survive client/process restart and report truthful progress.
- Candidate discovery does not load payload BLOBs.
- Old pending-selection APIs are removed from production paths.

## Rollout and rollback

Migrate legacy OCR rows into jobs based on status: ready/skipped remain domain terminal with no queued job; pending becomes queued; failed maps according to retry policy. During one release, commands can inspect both but only jobs execute. Rollback can stop workers and leave job tables; old OCR results remain readable. Do not run legacy and new workers concurrently.
