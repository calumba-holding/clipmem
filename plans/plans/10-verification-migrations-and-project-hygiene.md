---
status: implemented
created: 2026-07-10
last-verified: 2026-07-11
implemented-in: schema v23 verification and hygiene gates
owners: []
---

# Plan 10 — Verification, migrations, and project hygiene

**Priority:** P2 continuous gate  
**Primary owners:** all maintainers/CI/release/docs  
**Depends on:** runs alongside every plan

## Problem

The repository has substantial tests, but several architectural invariants are absent. Historical plans lack lifecycle status, architecture docs conflict with implemented image behavior, and performance tests do not enforce the cost boundaries identified by this audit. The supplied archive also lacked Git metadata needed by one script; tooling assumptions should be explicit.

## Required outcome

Correctness, migration, query cost, retrieval quality, job concurrency, and app-state contracts are automated. Plans/docs accurately reflect lifecycle. A weaker implementation model gets deterministic gates rather than judgment calls.

## Test architecture

### 1. Invariant suite

Create one reusable archive invariant checker used by tests and `doctor --verify-invariants`:

- SQLite FK check;
- representation→item existence;
- item/snapshot counts and byte totals;
- source fingerprint recomputation (sample/full mode);
- canonical document source fingerprint/builder readiness;
- FTS row/document parity;
- OCR/job/result consistency;
- derivative source references/hash verification;
- revision rows valid/nondecreasing;
- no expired active leases/restore operations beyond cleanup tolerance.

Report structured findings with severity and repairability. Read-only verify never repairs.

### 2. Migration matrix

Maintain fixture DBs for every supported schema era or representative boundary (0, major projection/FTS changes, image metadata, restore suppression, deferral, revisions, and each new plan schema). For each:

1. copy fixture;
2. migrate explicitly;
3. run invariants;
4. run representative search/get/export/restore-plan queries;
5. reopen read-only current;
6. ensure second migration is idempotent/no-op;
7. verify expected data/provenance.

Add failure fixtures: newer schema, unrelated SQLite, orphan representation, interrupted backfill, leased job, legacy compressed image.

### 3. Query-cost contracts

Use rusqlite trace/profile or an injectable connection observer in tests to assert:

- no schema DDL/immediate transaction on read opens;
- no `blob_value` on metadata/list/status/revision;
- bounded query count for list/get;
- indexed query plans for search cursors/job claims/filter stats;
- payload reads target the requested representation.

Avoid brittle full-SQL string snapshots; assert semantic categories/tables/columns and query-plan properties.

### 4. Retrieval evaluation

Extend `tests/search_benchmark.rs` with:

- mixed native/OCR duplicates;
- full multi-page expected ordering;
- weak irrelevant FTS hits;
- historical app cases;
- placeholder-only images;
- exact URL/path/punctuation;
- ties and corpus size scaling.

Save machine-readable quality/latency report as CI artifact. Set conservative regression thresholds only after stable repeated measurements; initially require explicit diff review.

### 5. Concurrency/failure injection

File-backed SQLite tests with barriers/fake clocks for:

- two capture contenders/restore operation;
- two job workers;
- lease expiry/stale completion;
- migration/backfill interruption;
- export disk/write failure;
- revision/document atomicity;
- service reconnect/cancel if plan 7 ships.

Expose narrow failpoints under test configuration rather than relying on timing sleeps.

### 6. Native app tests

Continue Swift Testing suite and add transport mocks/process fixtures for plan 8. CI should build/test supported macOS versions/architectures as release policy requires. Record app performance metrics in a dedicated opt-in benchmark, not flaky unit-test thresholds.

## Documentation/plan lifecycle

- Add status front matter described in `05-existing-plan-disposition.md`.
- Archive/mark six existing plans.
- Every new plan implementation PR updates its status and acceptance checklist.
- Add architecture decision records for:
  - unified search document/rank confidence;
  - source-preserving image semantics;
  - daemon gate decision;
  - trigger reduction/application-service ownership.
- Update architecture/user docs in the same release as behavior changes.
- Generate or test command/reference snippets against CLI help/schema where feasible to prevent drift.
- Make script prerequisites explicit. `check_file_lengths.py` may intentionally require Git tracked files; print a clear error when `.git` is absent or offer an archive-mode file list if release tarballs must support it.

## CI/release gates

Required on Linux/macOS as applicable:

- format, Clippy warnings denied, all Rust targets/tests;
- version and file-length checks;
- migration matrix/invariants;
- Xcode build/tests;
- skill parity and ClawHub check where tool/credentials exist;
- package/install smoke tests;
- retrieval benchmark artifact and reviewed diff for relevant changes.

Add a “source semantics” release test: seed fixtures, run all maintenance, export all source reps, compare hashes/fingerprints.

## Implementation sequence

1. Land invariant checker and baseline report against current fixtures; encode known failures as failing tests associated with P0/P1 plans, not permanent ignores.
2. Build migration fixture matrix and helper.
3. Add query trace/cost test harness.
4. Extend retrieval benchmark and save JSON artifact.
5. Add deterministic concurrency/failpoint helpers.
6. Expand Swift transport/state tests.
7. Add plan lifecycle/status tooling/check.
8. Update existing plan statuses and stale architecture docs when corresponding designs land.
9. Integrate gates incrementally so CI failures point to one clear contract.

## Edge cases

- Benchmarks vary by runner; separate correctness ordering from latency thresholds and use broad regression bands/repeated samples.
- Large fixture binaries can bloat repo; generate deterministic images/DBs in tests where practical, keep only minimal binary fixtures.
- Invariant full fingerprint scan may be expensive; doctor supports sample/quick/full modes, tests use full small fixtures.
- Migration failure must preserve original fixture/file; test on copies and production migration backup/transaction policy.
- ClawHub network/tool absence should skip only the publish-sync check with clear status, not silently pass parity.

## Acceptance criteria

- Every P0/P1 finding has at least one regression test that would fail on current behavior and pass after its plan.
- All supported migration fixtures pass invariants and idempotent reopen.
- CI can detect accidental BLOB reads and schema writes on read paths.
- Retrieval changes produce reviewable quality/latency artifacts.
- Existing plans are unmistakably implemented/superseded, and new plans carry lifecycle metadata.
- Architecture and user docs no longer contradict source/image/capture semantics.

## Rollout

Introduce gates in warn/report mode where baselines are noisy, then make them required once stabilized. Never leave a correctness invariant permanently nonblocking merely because the current code fails it; attach it to the implementing plan and flip at merge.
