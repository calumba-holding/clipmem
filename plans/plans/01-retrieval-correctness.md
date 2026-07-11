---
status: implemented
created: 2026-07-10
last-verified: 2026-07-11
implemented-in: schema v23 merged implementation
owners: []
---

# Plan 1 — Retrieval correctness

**Priority:** P0  
**Primary owners:** Rust database read layer and CLI retrieval layer  
**Depends on:** none  
**Enables:** canonical projection consolidation, reliable native UI pagination, meaningful recall evaluation

## Problem and evidence

- FTS BM25 values are negative, but current normalization clamps them to zero, producing `1.0` for every hit (`src/cli/commands/retrieval/recall.rs:370-373`).
- Native and OCR result streams are independently paginated and then merged with one cursor (`src/db/read/search.rs:223-365`; `src/db/read/search_results.rs:15-62`).
- Duplicate source rows lose OCR evidence.
- `--has-text` and Swift `hasText` treat display placeholders as text (`src/db/read/filter_sql.rs:269-286`; `macos/ClipmemMenuBar/ClipmemMenuBar/Models/ClipmemModels.swift:252-254`).
- Historical app filtering/search explanations are inconsistent.
- Hard-coded query expansions are not justified by the benchmark (`recall.rs:376-401`).

## Required outcome

One query yields one deduplicated, totally ordered snapshot stream. Pagination is applied after dedupe on that final ordering. Search output reports structured match evidence. Recall selection uses an explicitly tested match-quality contract rather than treating raw BM25 as confidence.

## Scope

### In scope

- Unified native + OCR search document/index sufficient for search and recall.
- FTS and literal modes.
- Search cursor version migration.
- Structured match evidence and provenance.
- Correct capability flags, especially text.
- Recall score/confidence and weak-match behavior.
- Removal or evidence-backed replacement of hard-coded expansions.
- Benchmark and adversarial pagination corpus.

### Out of scope

- Semantic/vector search.
- Full HTML/RTF parser work (plan 9).
- Broad projection-cache removal (plan 4), though this plan must establish the search-document nucleus plan 4 will own.
- Native app UI redesign.

## Design decisions

### 1. Introduce one row per snapshot for retrieval

Create a versioned `snapshot_search_documents` table (the final name may become `snapshot_documents` in plan 4) containing at minimum:

- `snapshot_id` primary key/FK;
- `builder_version`;
- `native_text`, `preview_text`, `ocr_text`;
- normalized URL/file-path text;
- historical and last app names/bundle IDs according to the chosen contract;
- capability booleans (`has_native_text`, `has_ocr_text`, `has_url`, `has_file_url`, `has_image`, `has_pdf`);
- last-observed timestamp and any deterministic tie-break fields needed by search.

Create one FTS5 table with separate columns for native text, preview, OCR, URL/path, and app metadata. Use column weights in one `bm25()` call. One FTS row means one snapshot; OCR is no longer a separate candidate stream.

Literal mode must query one unified haystack/document row, including OCR and the same app metadata contract. A trigram index may remain.

Plan 4 will make this the canonical document for all read surfaces. Do not create a second projection in plan 4.

### 2. Define final ordering and cursor

FTS final order:

```text
raw_bm25 ASC,
exact_phrase_match DESC,
last_observed_at DESC,
snapshot_id DESC
```

If exact-phrase adjustment is incorporated into a numeric rank, store/use that exact final numeric key consistently. Do not subtract arbitrary values in one query and normalize elsewhere without tests.

Literal final order:

```text
literal_match_tier DESC,
literal_detail_score DESC,
last_observed_at DESC,
snapshot_id DESC
```

The cursor encodes:

- schema/cursor version;
- query hash and normalized filter hash;
- mode used;
- all final ordering keys;
- snapshot ID.

The next-page predicate exactly mirrors lexicographic ordering. Pagination is performed in the single final query, so no emitted-ID memory is required.

Old cursors should fail with a clear “cursor version no longer supported; rerun without cursor” error rather than produce incorrect pages.

### 3. Separate rank from match quality

Internal `raw_rank` remains a database ordering value and is not serialized as confidence.

Add `MatchEvidence`:

- matched fields (set, not one source winner);
- native/OCR/app/URL provenance;
- exact phrase indicator;
- normalized simple-query term coverage where computable;
- snippet source and text;
- FTS mode/query complexity classification.

Define v1 user-facing match quality for **simple lexical queries** from evidence, not BM25 magnitude:

- `0.96`: exact normalized phrase in a matched searchable field;
- `0.86`: all normalized query terms present in one field;
- `0.80`: all terms present across matched fields;
- `0.68`: at least 60% of terms present;
- `0.50`: lower coverage but valid FTS/literal match.

Apply at most a `0.05` rank-position reduction across the first ten results, only to break quality ties; do not include recency or preferred-app bonuses in `match_quality`.

For complex explicit FTS syntax where term coverage is not meaningful, emit a nullable numeric quality and an ordinal confidence of `query_match` rather than fabricating a calibrated number. If backward compatibility requires a number, use a documented sentinel derived only from evidence category and mark `score_semantics: "evidence_v1"` in the envelope.

Recall sort score may combine:

- match quality (dominant);
- explicit `prefer_app` bonus;
- explicit `prefer_recent` bonus;
- deterministic rank-order tie-break.

Weak search is true when there is no candidate with quality at/above the configured threshold. This must be tested independently from raw BM25.

### 4. Remove unvalidated expansions

Delete the product-specific expansions by default. Retain only normalization that is linguistically/mechanically obvious and benchmarked (for example whitespace/case handled by tokenizer). Any synonym feature must be a versioned data table with a named eval case per mapping and must not be mixed into generic code branches.

### 5. Make flags factual

`has_text = has_native_text OR has_ocr_text`. Preview placeholders never count. `kind=text` must have a separately documented meaning; decide whether URL/file-url are textual kinds or only `has_text` capabilities and make CLI/docs/Swift identical.

Historical app contract: recommended behavior is that free-text search includes distinct historical app names/bundles because app filters already do. Match evidence must name `historical_app` and ideally the matching event/app string. If the team instead chooses last-app-only, remove historical behavior from generic app filtering or explicitly name filters `ever-app` vs `last-app`.

## Implementation sequence

1. Add failing tests for BM25 normalization, weak-match fallback, image placeholder `has-text`, duplicate native/OCR evidence, and mixed-source pagination.
2. Define Rust types: `SnapshotSearchDocument`, `MatchEvidence`, `SearchOrderKey`, cursor vNext, and `MatchQuality`/nullable score semantics.
3. Add schema migration for unified search document and FTS/literal index; do not drop legacy tables.
4. Implement one builder using existing snapshot native projection, OCR cache, URL projection, and event app aggregate. Record builder version.
5. Backfill in bounded batches during migration or explicit post-migration rebuild. Migration must be restart-safe; if full backfill is too large for one transaction, create table + pending builder version and let an explicit maintenance command finish before switching reads.
6. Dual-write unified documents from capture, OCR completion/clear, representation mutation, event insert/update/delete, and purge/forget. Initially use explicit rebuild calls in application/store code; avoid adding another sprawling trigger set.
7. Implement unified FTS and literal SQL with one row per snapshot and exact cursor predicate.
8. Generate `MatchEvidence` in row mapping; merge no longer needed for new path.
9. Replace recall normalization/weakness logic with evidence v1; remove hard-coded expansions.
10. Switch CLI search/recall behind an internal feature flag or schema readiness check; compare legacy/new top results in tests/optional debug mode.
11. Update output schema documentation while keeping existing fields parseable. Add `score_semantics`/match evidence fields additively.
12. Remove legacy merged path only after benchmark and pagination gates; plan 4 later removes old tables/triggers.

## Edge cases and failure modes

- Empty/whitespace query remains validation error or recent recall according to current command contract.
- Invalid strict FTS syntax remains a clear error; auto mode may choose literal based on existing analysis.
- OCR pending/failed contributes no OCR text but status remains visible in document.
- A snapshot matching multiple fields returns unioned fields and one deterministic best snippet.
- Documents missing/stale during migration must not silently disappear. Either fall back to legacy read with a visible readiness state or block search with actionable rebuild guidance.
- Cursor filter/query mismatch remains rejected.
- Equal rank/timestamp rows are ordered by snapshot ID.
- Updating OCR or event app metadata changes document revision and invalidates cursors only according to documented snapshot-consistency semantics; ordinary cursors need not be stable across concurrent mutations, but must never loop or decode incorrectly.

## Tests

- Unit tests for quality tiers and complex-query nullable semantics.
- SQLite tests with distinct negative BM25 values proving scores are not constant.
- 100+ seeded mixed native/OCR snapshots; paginate all pages and compare IDs exactly to one unpaginated final-order query.
- Duplicate snapshot matching native + OCR + URL; evidence union and no duplicate.
- Ties across all order keys except ID.
- Placeholder-only image/binary/empty; `has-text=false` until OCR ready.
- Historical first-app and last-app search/filter cases.
- Migration/backfill from versions 0, 11, 13, 18 fixtures.
- Search benchmark: save before/after top-k quality and P50/P95 DB latency.

## Acceptance criteria

- No production code calls the old two-stream merge for current-schema search.
- Every result page is a prefix/slice of one deterministic total ordering; full pagination has no gaps/duplicates in adversarial tests.
- Distinct FTS relevance/evidence can produce distinct quality values; no blanket 1.0.
- Weak-match fallback is exercised by tests and works with default threshold.
- `--has-text` and Swift filter semantics agree with factual flags.
- Existing machine consumers can decode additive output changes.
- Benchmark quality meets or exceeds baseline on all required cases; any intentional difference is documented.

## Rollout and rollback

Keep legacy search tables/path for one release. Add a hidden/debug comparison command or test helper that reports top-ID/evidence differences. Rollback switches the read path flag; unified rows can remain. Do not drop legacy tables until plan 4 completes a migration after at least one stable release.
