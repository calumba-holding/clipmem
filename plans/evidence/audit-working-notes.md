# Clipmem audit working notes

Source: `/mnt/data/clipmem-audit/source/clipmem-main` (read-only during audit)
Deliverables: `/mnt/data/clipmem-audit/deliverables`

## Scope decisions
- Audit all Rust, Swift, SQL, shell/Python, workflows, docs, skills, tests, and historical plans.
- Security/privacy hardening is out of scope except where a mechanism directly causes correctness/performance behavior; do not make security recommendations.
- Historical plan files are evidence; current code/changelog determine implementation status.
- Do not modify source.

## Verified baseline
- Version sync passes: Rust and menu app are 0.5.6.
- Rust toolchain unavailable; cargo fmt/test/clippy could not run.
- Linux Swift exists but AppKit/Xcode toolchain unavailable, so macOS app build/tests cannot run.
- ClawHub sync cannot run because external `clawhub` command is absent.
- File-length script depends on `.git`, absent from archive; retry in a temporary git repo or replicate manually.

## High-confidence findings in progress
1. Capture orchestration is divergent across watch, capture-once, and setup seed. Pause/ignore/OCR/retention/restore rules differ.
2. macOS pasteboard capture does not verify changeCount after reading; a mutation during enumeration can produce a torn/misattributed snapshot.
3. Every Database open runs full schema SQL under an IMMEDIATE transaction; routine reads therefore acquire write intent and replay 858 lines of DDL/migration checks.
4. Snapshot detail/projection loads every representation BLOB even though serialized detail omits raw bytes. This is especially costly for image/PDF snapshots and Swift UI subprocess calls.
5. OCR candidates are selected but never claimed/leased in the DB; concurrent processes can OCR the same image and race result writes.
6. Restore suppression is consumed in a separate transaction before storing. Multiple watchers (or capture-once plus watcher) can let one process consume the marker and another record the restore.
7. Search merges independently scored native and OCR result sets but paginates with one cursor score; differing score scales and duplicate rows can skip OCR/native results across pages. Dedupe always retains native explanation.
8. `--has-text` SQL treats any non-empty preview as text. Binary/image/empty snapshots receive non-empty bracketed previews, so the filter can include non-text content.
9. Model classification and text projection are duplicated between live builders and migration rebuild code, creating drift risk. Snapshot detail recomputes another projection again.
10. Trigger-heavy cache maintenance duplicates large SQL fragments and makes schema/mutation behavior hard to reason about; validate invariants before proposing replacement.
11. Existing plan files appear implemented but lack lifecycle status; stale plans can mislead future agents.

## Important design observations
- Core archive model: content-addressed snapshot -> ordered items -> UTI-keyed representations; capture events record observations and dedupe snapshots.
- Derived read model: snapshot_stats, projection cache, event filter cache, literal cache, OCR cache, plus five FTS tables.
- Image optimization mutates representation bytes/UTI and therefore recomputes snapshot fingerprints; inspect conflict/merge semantics closely.
- Retention intentionally deletes snapshots by last_observed_at (documented), not individual old events.
12. Image optimization intentionally replaces source bytes/UTI/hash and rewrites the snapshot fingerprint. This conflicts with the stronger exact-restoration/raw-representation product language and destabilizes content identity across optimization/capture boundaries.
13. The native app shells out once per operation and polls `service revision` every two seconds; each invocation currently runs schema preparation under write intent. This is a cross-process architectural hotspot, not just a local micro-optimization.
14. Chromium-origin inference overwrites the actual frontmost app for ChatGPT/Codex. Ignore policy then checks the fabricated `org.chromium.browser` identity, so ignoring `com.openai.chat` or `com.openai.codex` can fail for copies carrying Chromium metadata. Model observed app and inferred content origin separately.
15. The watcher handles capture, policy, OCR enqueue/worker, retention, revision notification, and logging in one command function. The same sequences are partially reimplemented elsewhere; this supports an application-service boundary rather than merely extracting helpers.

## Final synthesis and disposition
- Confirmed 38 findings, separated into correctness defects, performance/UX defects, architectural trade-offs, and project-hygiene issues in `deliverables/02-findings.md`.
- Executable SQLite probes confirmed: FTS5 negative BM25 ranks; placeholder-only `has-text`; orphan representation acceptance and silent hiding; historical app filter/literal mismatch; one-shot restore suppression; trigger/cache population.
- Preserve: snapshot/item/representation source model, separate capture events, raw UTI bytes, SQLite, CLI automation contract, and revisions as an invalidation concept.
- Replace incrementally: ad-hoc capture/restore orchestration, every-open migrations, BLOB-hydrating metadata reads, independent native/OCR pagination, process-local worker ownership, broad projection ownership, and in-place image source mutation.
- Delivery order fixed: retrieval correctness; capture/restore consistency; DB open/read cost boundaries; canonical projections/integrity; durable jobs; source-preserving image storage; measured service gate; native UX; text contracts; verification/hygiene.
- A daemon/service rewrite is explicitly gated until P0 work is complete and measured.
- Historical plans are mostly implemented; lifecycle/status changes and selective supersession are documented in `deliverables/05-existing-plan-disposition.md`.
- Source tree remained untouched; all audit outputs live outside it.
