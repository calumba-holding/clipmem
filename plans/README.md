# Clipmem architecture audit and implementation plans

Audit date: 2026-07-10  
Audited source: `clipmem` 0.5.6 from the supplied archive  
Scope: Rust CLI/library, SQLite schema and migrations, macOS capture/restore/OCR, Swift menu bar app, tests, scripts, workflows, documentation, skill packages, and historical plans. Security is intentionally out of scope.

The source archive had no `.git` directory, so this report identifies the input by product version and file contents rather than claiming a commit hash.

## Read this first

1. [`00-executive-audit.md`](00-executive-audit.md) — conclusions, priority order, and recommended delivery sequence.
2. [`01-system-map-and-coverage.md`](01-system-map-and-coverage.md) — end-to-end architecture, control/data flows, seams, load-bearing components, and module coverage.
3. [`02-findings.md`](02-findings.md) — evidence-backed bugs, design problems, trade-offs, and lower-priority improvements.
4. [`03-target-architecture.md`](03-target-architecture.md) — an incremental target design that preserves useful product semantics without preserving accidental structure.
5. [`04-verification.md`](04-verification.md) — checks run, executable SQL probes, constraints, and missing test contracts.
6. [`05-existing-plan-disposition.md`](05-existing-plan-disposition.md) — keep, amend, supersede, or archive decisions for the existing plans.
7. [`plans/00-roadmap.md`](plans/00-roadmap.md) — dependency-ordered handoff roadmap.

## Implementation plans

| Order | Plan | Priority | Purpose |
|---:|---|---|---|
| 1 | [`01-retrieval-correctness.md`](plans/01-retrieval-correctness.md) | P0 | Fix recall scoring, merged search pagination/provenance, and filter semantics. |
| 2 | [`02-capture-and-restore-consistency.md`](plans/02-capture-and-restore-consistency.md) | P0 | Create one capture policy pipeline, stable pasteboard reads, race-safe restore suppression, and non-destructive restore. |
| 3 | [`03-database-open-and-targeted-read-paths.md`](plans/03-database-open-and-targeted-read-paths.md) | P0 | Stop routine reads taking schema write intent and stop loading unrelated BLOBs. |
| 4 | [`04-schema-integrity-and-canonical-projections.md`](plans/04-schema-integrity-and-canonical-projections.md) | P1 | Enforce item/representation integrity and establish one canonical derived document/projection contract. |
| 5 | [`05-durable-background-jobs.md`](plans/05-durable-background-jobs.md) | P1 | Add atomic claims, leases, retries, and resumability for OCR and image work. |
| 6 | [`06-source-preserving-image-storage.md`](plans/06-source-preserving-image-storage.md) | P1 | Preserve exact source bytes and identity while supporting efficient image derivatives/storage. |
| 7 | [`07-long-lived-service-boundary.md`](plans/07-long-lived-service-boundary.md) | P1/P2 gate | Replace high-frequency subprocess/polling only after measured prerequisites are complete. |
| 8 | [`08-native-app-resilience-and-ux.md`](plans/08-native-app-resilience-and-ux.md) | P2 | Make app refresh, cancellation, previews, selection, and copy semantics robust. |
| 9 | [`09-text-projection-and-content-contracts.md`](plans/09-text-projection-and-content-contracts.md) | P2 | Improve HTML/RTF/binary handling and align search, detail, and filters. |
| 10 | [`10-verification-migrations-and-project-hygiene.md`](plans/10-verification-migrations-and-project-hygiene.md) | P2 | Add invariant tests, migration gates, benchmarks, plan lifecycle, and documentation maintenance. |

## Supporting artifacts

The bundled [`evidence/`](evidence/) directory contains the executable SQLite probe and raw results, BM25 output, unavailable-tool logs, source provenance, the full file inventory, Rust/Swift API maps, symbol inventory, key-source index, and the working decision log. Start with:

- [`source-provenance.txt`](evidence/source-provenance.txt) — exact input ZIP SHA-256 and audited counts.
- [`sqlite_invariant_probes.py`](evidence/sqlite_invariant_probes.py) and [`sqlite-invariant-probes.json`](evidence/sqlite-invariant-probes.json) — reproducible schema/invariant checks.
- [`fts-bm25-probe.txt`](evidence/fts-bm25-probe.txt) — raw FTS5 ranking evidence for F-01.
- [`all-text-files.txt`](evidence/all-text-files.txt), [`rust-api-map.txt`](evidence/rust-api-map.txt), and [`swift-api-map.txt`](evidence/swift-api-map.txt) — coverage and API inventories.
- [`audit-working-notes.md`](evidence/audit-working-notes.md) — findings and decisions recorded during the audit.
- [`deliverable-validation.txt`](evidence/deliverable-validation.txt) and [`source-immutability-check.txt`](evidence/source-immutability-check.txt) — final link/citation checks and proof the audited source matches a fresh extraction.

The original source was not modified.
