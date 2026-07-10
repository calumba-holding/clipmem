# Findings

Severity labels describe user/system impact, not exploitability. Security is out of scope.

## P0 — correctness and data-safety

### F-01 — FTS recall normalization collapses all normal FTS hits to 1.0

**Evidence.** `normalize_fts_score` takes `score.unwrap_or_default().max(0.0)` and returns `1.0 / (1.0 + value)` (`src/cli/commands/retrieval/recall.rs:370-373`). SQLite FTS5 `bm25()` in the current queries returns negative lower-is-better values. The executable probe in [`evidence/fts-bm25-probe.txt`](evidence/fts-bm25-probe.txt) produced distinct negative ranks.

**Impact.** Confidence is false, `--min-score` does not mean what it claims for FTS, weak-search expansion/recent fallback is suppressed (`recall.rs:155-203`), and relevance tuning is obscured.

**Recommendation.** Do not invent a confidence transform directly from raw BM25. First preserve rank/order; then define a tested, query-relative score or a calibrated feature model. Until calibration exists, expose `match_quality` categories derived from explicit evidence and rank, not a pseudo-probability.

### F-02 — merged native/OCR search pagination can skip results

**Evidence.** Native and OCR searches independently apply cursor and `limit + 1`, then `merge_scored_search_results` deduplicates, sorts, and truncates (`src/db/read/search.rs:223-365`; `src/db/read/search_results.rs:15-62`). One scalar score from the merged last row is reused for both sources in the next cursor.

**Impact.** Different score distributions, source-specific ordering, and post-fetch dedupe mean unseen rows can fall behind the wrong source cursor or be consumed by duplicates. `has_more` can also be true without a reliable next page boundary.

**Recommendation.** Preferred: build one snapshot search document/index with native and OCR fields and paginate one ordered stream. Transitional alternative: encode separate native and OCR cursor states plus a deterministic merge watermark and provenance-aware dedupe.

### F-03 — duplicate native/OCR matches lose OCR provenance

**Evidence.** The merge inserts native rows first and ignores an OCR row with the same snapshot ID (`search_results.rs:15-37`). It does not combine `matched_fields`, snippets, or explanations.

**Impact.** A result that genuinely matched OCR may be explained as a weaker or unrelated native match. This affects user trust and recall feature scoring.

**Recommendation.** Merge evidence, not just rows: union matched fields, select the best snippet by query evidence, retain source-specific scores internally, and generate one explanation after dedupe.

### F-04 — capture policy differs across watch, capture-once, and setup seed

**Evidence.** Watch checks pause/ignored apps and runs OCR/retention/notifications (`src/cli/commands/runtime.rs:32-159`). Capture-once omits pause/ignored-app and OCR (`src/cli/commands/runtime.rs:206-252`). Setup seed calls `store_capture_if_allowed` directly (`src/cli/service/manage.rs:236-249`). Documentation says policy applies to all capture methods (`docs/managing-your-archive.md:125-128`).

**Impact.** Paused or ignored content can be captured manually/setup; OCR behavior is surprising; future policy additions will drift further.

**Recommendation.** One `CaptureApplicationService::ingest` with an explicit `CaptureMode` and documented policy matrix. Mode-specific differences must be named flags, not omitted calls.

### F-05 — pasteboard capture can be torn and app attribution can be stale

**Evidence.** The macOS adapter reads change count once, reads frontmost app, then enumerates all pasteboard items and representations, without re-reading change count (`src/platform/macos.rs:26-60`).

**Impact.** If the pasteboard changes during enumeration, one snapshot can mix representations/states or carry the wrong change count/app.

**Recommendation.** Bounded stable-read loop: read generation, capture items and observed app, read generation again, accept only if equal; retry a small number with jitter, then report a transient capture outcome without storing.

### F-06 — restore suppression is one-shot and not safe with multiple capture processes

**Evidence.** `store_watched_capture_if_allowed` consumes the marker in a separate transaction before attempting store (`src/db/store/capture.rs:132-190`). The `BEFORE INSERT` trigger also deletes a matching marker and raises ignore only for that insertion (`src/db/schema.sql:314-324`). The executable probe showed first insert suppressed and marker deleted; second insert stored.

**Impact.** Two watchers, or watcher plus manual capture, can record the restore the feature is meant to suppress.

**Recommendation.** Tie suppression to a restore operation ID plus expected pasteboard generation/hash and an expiry. Check suppression and capture insertion in one transaction, but do not consume the operation for only one contender; expire it when the pasteboard generation advances or after a safe deadline.

### F-07 — restore clears the current clipboard before validating the replacement

**Evidence.** `restore_items` calls `clearContents()` before constructing all `NSPasteboardItem` objects and setting all representations (`src/platform/macos.rs:63-92`).

**Impact.** Unsupported/invalid data can erase the user's current clipboard on a failed restore.

**Recommendation.** Build and validate every destination item before clearing. Capture a best-effort rollback representation of the current pasteboard; after clear, if write fails, attempt rollback and return a structured error reporting both primary and rollback results.

### F-08 — `--has-text` includes image/binary placeholders

**Evidence.** SQL considers non-empty snapshot/item preview text a text signal (`src/db/read/filter_sql.rs:269-286`). Builders deliberately create placeholders such as `[image · N bytes]`. Swift similarly treats any non-empty preview as text (`macos/ClipmemMenuBar/ClipmemMenuBar/Models/ClipmemModels.swift:252-254`). The SQL probe confirmed an image-only snapshot matches the expression.

**Impact.** Filter semantics are wrong in CLI and UI; recall's recent-candidate text bonus can reward placeholders (`recall.rs:296-331`).

**Recommendation.** Persist canonical capability flags (`has_native_text`, `has_ocr_text`, `has_url`, etc.) from representation/OCR facts. Never infer text presence from display preview.

### F-09 — representation rows can reference nonexistent items

**Evidence.** `item_representations` references only `snapshots(id)` (`src/db/schema.sql:25-41`). The item table has composite key `(snapshot_id,item_index)`, but there is no composite FK. `load_snapshot_items` groups reps under loaded items and silently ignores unmatched reps (`src/db/read/snapshot.rs:280-357`). The executable probe confirmed insertion is allowed and `foreign_key_check` stays clean.

**Impact.** Partial/corrupt state is invisible to normal reads, projections, restore, and export.

**Recommendation.** Add `FOREIGN KEY(snapshot_id,item_index) REFERENCES snapshot_items(snapshot_id,item_index) ON DELETE CASCADE`; migrate with preflight detection and explicit repair/quarantine policy.

### F-10 — forced export is not failure-atomic

**Evidence.** `create_export_destination` removes the existing path before creating/writing the new file (`src/cli/commands/archive_mutate.rs:154-197`).

**Impact.** A crash, disk-full condition, or write error destroys the previous destination and can leave a partial file.

**Recommendation.** Write to a uniquely named temporary file in the destination directory, flush/sync as appropriate, set permissions, then atomically rename over the destination. Clean up temp files on all failures.

## P0/P1 — systemic performance and operational correctness

### F-11 — every database open performs schema preparation under write intent

**Evidence.** Both open methods use read-write flags and call `prepare_connection` (`src/db/core.rs:24-76`, `src/db/core.rs:224-227`). `prepare_schema` starts `TransactionBehavior::Immediate` and executes the full schema every time (`src/db/schema.rs:16-52`). The busy timeout is 1.5 seconds (`src/db/core.rs:242-252`).

**Impact.** Read commands contend with writers/maintenance, app polling repeatedly takes write intent, and errors can surface as setup/locking problems unrelated to the requested read.

**Recommendation.** Three explicit modes: read-only/current, read-write/current, and migrate/init. Fast current-schema validation should read `user_version` without DDL. Only lifecycle commands migrate.

### F-12 — read APIs hydrate unrelated BLOBs and list projections are N+1

**Evidence.** `find_snapshot` and `snapshot_projection` call `load_snapshot_items` (`src/db/read/snapshot.rs:18-60`), whose representation query selects BLOB values (`snapshot.rs:280-357`). List output loops unique IDs and calls projection per snapshot (`src/cli/commands/retrieval_support.rs:29-51`).

**Impact.** Large images/PDFs amplify memory, I/O, and subprocess latency for search, get, export validation, and app detail.

**Recommendation.** Separate `SnapshotMetadata`, `SnapshotPayloadManifest`, and targeted `RepresentationPayload`. Query projections in one set-based statement. Make a BLOB read impossible unless the method name/type asks for it.

### F-13 — native app multiplies process/database overhead

**Evidence.** `ClipmemClient` starts a CLI process per operation (`macos/ClipmemMenuBar/ClipmemMenuBar/Services/ClipmemClient/ClipmemClient.swift:179-226`). Startup refresh launches status/settings/recent concurrently (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:93-100`); a revision command runs every two seconds (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:575-636`); pasteboard and Darwin monitors can trigger more refreshes.

**Impact.** Process churn and schema-open contention dominate lightweight UI reads. Parallel startup commands can contend with one another.

**Recommendation.** First complete F-11/F-12. Then measure. If still material, introduce one long-lived local coordinator/event stream while retaining CLI compatibility.

### F-14 — image detail reads the archive repeatedly for one preview

**Evidence.** Detail `get` causes full snapshot hydration, then `loadImagePreview` invokes `export` (`SnapshotDetailView.swift:222-264`), and export itself first calls `find_snapshot` then a target-byte query (`archive_mutate.rs:24-61`).

**Impact.** At least two all-representation loads plus target read and temporary-file round trip.

**Recommendation.** Metadata-only `get`; direct targeted preview endpoint/stream keyed by snapshot/item/UTI and content hash. Cache preview by content version, not snapshot ID alone.

## P1 — background work, identity, and derived-state ownership

### F-15 — OCR jobs are selected but never claimed

**Evidence.** `next_ocr_candidates` selects pending rows and commits without changing them to an owned/leased state (`src/db/store/ocr.rs:28-128`). The worker guard is process-local.

**Impact.** Multiple CLI/app/watch processes can perform the same expensive OCR and race writes; crashes have no explicit lease recovery semantics.

**Recommendation.** Durable job table/state with atomic claim (`UPDATE ... RETURNING` or transactional select/update), owner, lease expiry, attempts, not-before, algorithm version, and idempotent completion.

### F-16 — optimizer has the same ownership/resumability gap

**Evidence.** Candidates are loaded in batches and mutated row by row, without claim/lease (`src/db/store/optimize.rs:318-351`). A callback/command failure can leave a partially completed batch; another process can choose the same rows. Skip updates are not revisioned (`optimize.rs:464-490`).

**Impact.** Duplicate encoding work, races, opaque partial progress, and UI state that may not refresh.

**Recommendation.** Reuse the durable job protocol; report operation ID and durable counters; make cancellation stop claiming new work while allowing claimed work to complete or lease-expire.

### F-17 — image optimization mutates the logical source and snapshot identity

**Evidence.** Successful optimization rewrites UTI/BLOB/hash/status and recomputes snapshot fingerprint (`src/db/store/optimize.rs:532-664`). It decodes to RGBA before re-encoding (`optimize.rs:389-449`). Tests assert decoded pixel equality, not source-container equivalence (`src/db/tests/image_and_perf.rs:20-108`).

**Impact.** Exact raw export/restore semantics, metadata/color/orientation/bit-depth fidelity, and stable content addressing are weakened. Original future captures can deduplicate differently.

**Recommendation.** Source representations immutable. Store derivative renditions separately with algorithm/version/source hash. If disk reclamation requires source compression, use a reversible physical encoding while reproducing exact original bytes at the logical API.

### F-18 — optimizer batching can create large memory spikes

**Evidence.** Up to 250 full image BLOBs are loaded, ordered largest first (`optimize.rs:318-351`), then decoded to RGBA.

**Impact.** A batch of large screenshots can consume far more memory than stored size.

**Recommendation.** Claim one/few jobs, enforce byte-based in-flight budget, decode/process sequentially or with bounded concurrency, and expose per-job limits.

### F-19 — savings arithmetic can overflow on narrow/very large values

**Evidence.** Relative savings comparison multiplies `usize` operands without checked/widened arithmetic (`src/db/store/optimize.rs:451-462`).

**Impact.** Low probability on normal macOS images, but easy to remove and undesirable in storage decision code.

**Recommendation.** Use `u128`/checked arithmetic or rearranged division with documented rounding.

### F-20 — projection/classification logic has multiple owners

**Evidence.** Live builders classify and project (`src/model/builders.rs`, `text_projection.rs`); migration rebuild code repeats representation decoding/classification in `src/db/schema.rs`; snapshot detail computes another flattened projection (`src/db/read/snapshot.rs`); optimizer rebuilds item/snapshot summaries (`src/db/store/rebuild.rs`).

**Impact.** Migration, capture, search, and detail can disagree. Any new UTI/text rule requires edits across layers.

**Recommendation.** One versioned canonical `SnapshotDocumentBuilder` operating from a representation manifest/bytes adapter. Persist builder version and rebuild derived rows through one service.

### F-21 — large triggers create brittle order dependencies

**Evidence.** Schema triggers maintain stats, event filters, literal cache, projection indexes, OCR indexes, and representation projections across `src/db/schema.sql:211-858`. `snapshots_au` rewrites literal cache from only snapshot fields (`schema.sql:232-248`), while other paths later rebuild richer URL/app content. Projection-cache update only owns file-URL FTS (`schema.sql:268-286`).

**Impact.** Correctness depends on later trigger/manual rebuild order; direct updates can temporarily or permanently lose fields; migrations are difficult to prove.

**Recommendation.** Keep narrow integrity triggers if useful, but move multi-table document rebuild into one explicit transactional mutation service. Add invariant verification commands/tests.

### F-22 — historical app filtering and free-text app search disagree

**Evidence.** Event filter cache aggregates all historical apps; literal cache stores only last app fields. Executable probe showed app filter matched the first app while literal haystack contained only the second. Trigger/rebuild SQL reflects this split (`src/db/schema.sql:325-645`, `src/db/store/rebuild.rs:78-108`).

**Impact.** Users can filter by an old app identity but cannot find the same snapshot by typing that app name; match explanations only inspect the last app.

**Recommendation.** Decide the contract. Preferred: canonical document includes distinct historical app names/bundles and explanation identifies matching event metadata. If search should be last-app-only, document and align filter semantics.

### F-23 — observed app and inferred content origin are conflated

**Evidence.** When Chromium metadata is present and ChatGPT/Codex is frontmost, origin inference replaces the captured app with `org.chromium.browser` (`src/platform/macos.rs:126-150`; tests at `src/platform/macos.rs:342-363`). Ignore policy then sees the inferred identity.

**Impact.** Ignoring `com.openai.chat`/Codex may not work; analytics/history lose the actual frontmost app.

**Recommendation.** Store `observed_frontmost_app` and optional `content_origin_app` separately. Capture policy must use observed app; UI/search may expose either with explicit labels.

### F-24 — revision categories do not cover every visible mutation

**Evidence.** Image skip status changes without archive/storage revision (`optimize.rs:464-490`). Purge/forget can remove unreferenced OCR rows while bumping archive content but not OCR revision (`src/db/store/purge.rs`, command mutation paths).

**Impact.** Polling consumers can retain stale maintenance/OCR state; same snapshot ID can change content after optimization without a content-version key.

**Recommendation.** Define a mutation-to-revision matrix and test it. Every externally observable state transition must bump the relevant category in the same transaction. Include per-snapshot content/projection version for caches.

## P2 — UX, content fidelity, and maintainability

### F-25 — image preview cache key is only snapshot ID

**Evidence.** Detail preview task is keyed by `detail?.snapshotId` (`SnapshotDetailView.swift:56-59`). Optimizer can change the representation under the same ID.

**Impact.** Refreshed detail can continue showing stale bytes.

**Recommendation.** Key by `(snapshot_id, projection/content revision, item_index, UTI, raw_sha256/derivative_id)`.

### F-26 — transient recent-preview failures erase useful UI state

**Evidence.** `refreshRecentPreview` sets `recentPreview = []` on any error (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:132-142`).

**Impact.** A lock/process hiccup looks like data loss.

**Recommendation.** Stale-while-revalidate: retain rows, record refresh error/timestamp, show a non-destructive warning.

### F-27 — external history refresh discards loaded pages and may retain stale detail

**Evidence.** `refreshForExternalHistoryChange` reloads only cursor `nil` and replaces results (`HistoryModel.swift:82-125`). If selection remains, detail is not necessarily reloaded. Optimizer can mutate same ID.

**Impact.** Scroll/page state collapses; detail can disagree with row/archive.

**Recommendation.** Preserve loaded window or invalidate incrementally; reload selected detail when its content/projection revision changes.

### F-28 — HistoryModel ignores stale results but does not actively cancel the old subprocess

**Evidence.** Generation counters reject old results (`HistoryModel.swift:64-167`), but no request-task handle is retained/cancelled. Quick recall does retain/cancel a task (`QuickRecallModel.swift:30-36`).

**Impact.** Rapid query/filter changes leave obsolete CLI/database work running.

**Recommendation.** Own and cancel page/detail tasks. Ensure cancellation reaches `CommandRunner` and kills the process reliably.

### F-29 — command timeout is reported as cancellation and process termination is weak

**Evidence.** Timeout sets the same cancellation state and calls `Process.terminate()` (`CommandRunner.swift:28-91`). There is no distinct timeout error, escalation, or child process-group handling. Streaming catch can wait for a process after terminate (`CommandRunner.swift:94-155`).

**Impact.** Diagnostics are misleading; a stuck child can delay shutdown/cancellation.

**Recommendation.** Distinct timeout error, TERM grace period, KILL escalation, process-group ownership where appropriate, and tests with a TERM-ignoring child fixture.

### F-30 — self-ignore installation is not scoped to database configuration

**Evidence.** One global `didInstallSelfIgnore` default prevents reinstallation after DB override changes (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:542-550`).

**Impact.** The app may capture its own clipboard actions in a newly selected archive.

**Recommendation.** Make desired self-ignore idempotent on every active database or key the marker by canonical database identity/schema instance ID.

### F-31 — app preference changes can create databases as a side effect

**Evidence.** App settings revision propagation resolves previous/new DB paths and opens them with `Database::open_or_init` (`src/cli/commands/app.rs:103-130`, `src/cli/commands/app.rs:582-618`).

**Impact.** A typo/nonexistent override path can create an empty archive while merely changing preferences.

**Recommendation.** Notify app preferences through UserDefaults/Darwin/shared app channel; only bump revisions in archives that already exist and pass current-schema checks.

### F-32 — rich content “Copy” can silently flatten to plain text

**Evidence.** Detail copy uses `copyableDetailText` when available, otherwise exact restore (`SnapshotDetailView.swift:166-219`; `ClipmemModels.swift:312-317`). The separate Restore button also performs exact restore.

**Impact.** A rich HTML/RTF snapshot labeled “Copy” may lose formats, while users may expect clipboard fidelity.

**Recommendation.** Make labels explicit: “Copy text” and “Copy original”/“Restore original.” Consider exact copy as primary for rich snapshots.

### F-33 — HTML and RTF projection are intentionally shallow

**Evidence.** `src/model/text_projection.rs` performs lightweight extraction rather than a full HTML/RTF parser. Search/detail depend on these projections.

**Impact.** Script/style content, entities, RTF destinations/unicode, and malformed input can produce missing/noisy text.

**Recommendation.** Introduce bounded, deterministic parser adapters with fixtures from real clipboard producers; keep raw bytes authoritative and version the projection builder.

### F-34 — binary decoding and detail/search semantics diverge

**Evidence.** Representation construction can retain decoded text for bytes classified as binary, while searchable fragments filter by textual kind; detail flattening can include any `text_value` except selected metadata (`src/model/builders.rs`, `src/model/text_projection.rs`, `src/db/read/snapshot.rs`).

**Impact.** Detail can display text that search cannot find, or binary metadata can leak into summaries unpredictably.

**Recommendation.** Model decoding separately from semantic eligibility: `decoded_text`, `search_role`, `display_role`, `sensitivity_role`. One projection contract decides each use.

### F-35 — recall contains product-specific hard-coded query expansions

**Evidence.** `expanded_recall_queries` embeds phrases such as “half off,” “remote pytest,” and service permission variants (`src/cli/commands/retrieval/recall.rs:376-401`).

**Impact.** Generic retrieval behavior is surprising, brittle, and difficult to evaluate; it can favor unrelated content.

**Recommendation.** Remove until justified by benchmark data, or load a small versioned/configurable synonym set with per-expansion tests and telemetry-free offline evaluation.

### F-36 — setup seed capture is a hidden mutation with different semantics

**Evidence.** Setup captures the current clipboard as part of service initialization (`src/cli/service/manage.rs:21-32`, `manage.rs:236-249`).

**Impact.** Setup can archive content before the watcher policy is consistently applied and surprises users who expect only infrastructure setup.

**Recommendation.** Either remove seed capture or make it explicit in output/flag and route it through the common capture service.

### F-37 — historical plans have no lifecycle status

**Evidence.** Six detailed plan files remain under `docs/plans`, while current code already contains Markdown rendering/link handling/agent parity/image preview work. The files have no implemented/superseded front matter.

**Impact.** Future models may reimplement completed work or treat stale decisions as requirements.

**Recommendation.** Add required plan status/implemented-in/superseded-by fields and archive completed plans. See `05-existing-plan-disposition.md`.

### F-38 — architecture documentation is stale around image compression

**Evidence.** `docs/architecture.md:112-114` says image compression is not part of the phase, while current commands/schema/docs implement it (`docs/managing-your-archive.md:101-121`; `src/db/store/optimize.rs`).

**Impact.** The architecture contract is internally inconsistent, especially around raw-byte/exact restore claims.

**Recommendation.** Update only after plan 6 decides source semantics; document logical source vs derivative/physical representation explicitly.

## Things that are complex but not presently defects

- Filtered stats materialize temporary matching event/snapshot tables and run multiple aggregates. For an explicit CLI stats command this is a reasonable design; optimize only with measurements.
- The agent context command performs several aggregates. It is acceptable on demand; do not use it as a refresh endpoint.
- SQLite triggers are not inherently wrong. The issue is their current breadth, duplicated document construction, and untested ordering dependencies.
- Five FTS tables may be justified by tokenizer needs. The problem is independent candidate pagination and unclear document ownership, not merely table count.
