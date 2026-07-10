# System map and coverage

## Audit coverage

The supplied archive contains 277 files: approximately 41,482 Rust lines, 9,825 Swift lines, an 858-line SQL schema, integration tests, packaging scripts, four synchronized skill-package surfaces, release workflows, and documentation. The source archive was inspected without modification.

Coverage was tracked by subsystem rather than by sampling. Every production/test/support file was inventoried. Load-bearing paths were line-read and cross-checked against tests and documentation; repetitive UI composition and mirrored skill copies were structurally compared and traced to their owning generators/validators.

## Product topology

```text
macOS pasteboard
    │ capture_snapshot()
    ▼
normalized ClipboardSnapshot
    │ capture application policy
    ▼
SQLite authoritative archive
    ├── snapshots
    ├── snapshot_items
    ├── item_representations (raw bytes)
    └── capture_events
          │
          ├── trigger-maintained projections/stats/FTS
          ├── OCR queue + Vision worker
          ├── image optimizer + VACUUM
          └── archive revision categories

Consumers
    ├── Rust CLI: search/recent/timeline/recall/get/export/restore/admin
    ├── Swift menu bar app: one subprocess per operation
    └── agent skill packages/context commands
```

## Authoritative and derived data

### Authoritative payload/history

- `snapshots`: deduplicated snapshot identity, kind, preview/search text, totals.
- `snapshot_items`: ordered clipboard items and primary representation metadata.
- `item_representations`: one UTI representation and raw BLOB per item.
- `capture_events`: each observation, change count, and frontmost-app metadata.
- Settings and ignored bundle IDs are authoritative policy state.

The builder computes a versioned content fingerprint from item index, sorted UTI, and exact bytes (`src/model/builders.rs`; `src/model/clipboard.rs`). Repeated captures reuse the snapshot and add events (`src/db/store/capture.rs:27-113`).

### Derived/read-optimized state

- `snapshot_stats`: counts, first/last observations, last app.
- `snapshot_projection_cache`: URLs and file URLs.
- `snapshot_event_filter_cache`: all historical app/bundle values for filter acceleration.
- `snapshot_literal_cache`: flattened lowercased literal haystack.
- `snapshot_ocr_cache`: aggregate OCR text/status by snapshot.
- `archive_revisions`: category revisions for external invalidation.
- Five FTS5 tables: native text, native literal trigram, file URLs, OCR text, OCR literal trigram.

These are maintained by schema triggers plus explicit rebuild functions (`src/db/schema.sql:211-858`, `src/db/store/rebuild.rs`). This is the most load-bearing and difficult-to-reason-about seam in the current design.

## End-to-end control flows

### 1. Background capture

1. `watch` polls `NSPasteboard.changeCount` (`src/cli/commands/runtime.rs:32-82`).
2. `platform::macos::capture_snapshot` enumerates items/types and reads raw bytes, while also reading the frontmost app (`src/platform/macos.rs:26-60`).
3. Model builders classify UTIs, decode supported text, select primary reps, build previews/search text, and fingerprint exact bytes (`src/model/builders.rs`, `src/model/kinds.rs`, `src/model/text_projection.rs`).
4. Watch checks pause/ignored app settings, then `store_watched_capture_if_allowed` checks restore suppression and API-key policy before inserting/reusing the snapshot and adding the event (`src/cli/commands/runtime.rs:83-113`, `src/db/store/capture.rs:115-149`).
5. Watch may enqueue OCR, spawn a process-local worker, apply retention, bump revisions, and post a Darwin notification (`src/cli/commands/runtime.rs:100-159`).

**Seam:** steps 4–5 are not shared with `capture-once` or setup seed, so entry points have different policy semantics.

### 2. Search/recent/timeline/recall

1. CLI parses a shared `RetrievalFilters` shape and an optional opaque cursor (`src/cli/schema.rs`, `src/cli/commands/retrieval/cursor.rs`).
2. Query analysis chooses FTS or literal mode (`src/db/read/query_analysis.rs`, `src/db/read/search.rs:163-206`).
3. Native and OCR indexes execute independently and are merged (`src/db/read/search.rs:223-365`, `src/db/read/search_results.rs:15-62`).
4. List rows are mapped from snapshot/event/cache joins (`src/db/read/row_mapping.rs`, `src/db/read/queries.rs`).
5. CLI then loads a rich projection per unique snapshot (`src/cli/commands/retrieval_support.rs:29-51`), currently hydrating all representation BLOBs.
6. `recall` converts raw search scores into a “normalized” score, applies hand-authored bonuses/expansions, optionally adds recent candidates, and emits best + alternatives (`src/cli/commands/retrieval/recall.rs`).
7. Output adapters render human/text/JSON/Markdown/TOON from shared output models (`src/cli/output/*`, `src/cli/human/*`).

**Seams:** raw database rank → recall confidence; independent native/OCR rank spaces → one cursor; compact SQL row → expensive representation-derived projection.

### 3. Detail, export, copy, and restore

- `get` calls `find_snapshot`, which reads snapshot metadata, events, items, and all representation BLOBs; serialization omits those bytes (`src/db/read/snapshot.rs:18-33`, `src/model/clipboard.rs`).
- `export` validates existence through `find_snapshot`, then separately reads one target representation and writes it (`src/cli/commands/archive_mutate.rs:24-75`).
- `restore` reads the full snapshot, records a pending restore marker, and calls the platform writer (`archive_mutate.rs:78-151`, `src/platform/macos.rs:63-92`).
- Swift detail uses `get`; image preview then invokes `export` into a temporary file and decodes an `NSImage` (`macos/ClipmemMenuBar/ClipmemMenuBar/Views/SnapshotDetailView.swift:222-264`).

**Seams:** metadata vs payload access; archive transaction vs live pasteboard mutation; exact rich copy vs flattened plain-text copy semantics.

### 4. OCR

1. Snapshot capture or `ocr run` inserts `ocr_results` rows keyed by representation raw SHA (`src/db/store/ocr.rs:10-25`).
2. `next_ocr_candidates` requeues eligible failures and selects pending rows with image payloads (`src/db/store/ocr.rs:28-128`). It does not claim them.
3. Apple Vision processes each candidate on macOS (`src/ocr/macos.rs`, `src/ocr.rs`).
4. Success/failure updates the shared raw-hash result, rebuilds affected snapshot OCR caches, updates FTS via triggers, and bumps OCR revision (`src/db/store/ocr.rs:131-203`, `src/db/store/rebuild.rs`).

**Seam:** process-local worker coordination vs database-global job ownership.

### 5. Image optimization

1. Candidate rows are uncompressed image representations, loaded in batches of 250, largest first (`src/db/store/optimize.rs:318-351`).
2. The image crate decodes to RGBA and encodes lossless WebP (`optimize.rs:389-449`).
3. Savings/conflicts are evaluated.
4. A successful row is replaced in place, OCR is copied to the new raw hash, item/snapshot projections and fingerprint are rebuilt, and archive/storage revisions are bumped (`optimize.rs:532-664`).
5. A skip updates status/reason without a revision (`optimize.rs:464-490`).
6. Optional compaction checkpoints and vacuums SQLite (`src/db/core.rs`, `src/cli/commands/storage.rs`).

**Seams:** logical source representation vs physical storage optimization; durable batch progress vs process lifetime.

### 6. Native app

1. `ClipmemClient` resolves the CLI binary, injects `--db`, runs a subprocess, maps exit codes, and decodes snake_case JSON (`macos/ClipmemMenuBar/ClipmemMenuBar/Services/ClipmemClient/ClipmemClient.swift:73-251`; `macos/ClipmemMenuBar/ClipmemMenuBar/Services/ClipmemClient/ClipmemCommand.swift`).
2. `AppModel.start` configures launch-at-login, installs a self-ignore, runs status/settings/recent concurrently, starts a pasteboard poller, Darwin notification listener, two-second archive revision poll, and update checker (`macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:78-100`, `macos/ClipmemMenuBar/ClipmemMenuBar/ViewModels/AppModel.swift:542-636`).
3. `HistoryModel` translates UI state into recall/search/recent/timeline requests, paginates, and loads detail (`HistoryModel.swift:31-258`).
4. `QuickRecallModel` debounces and cancels the active Swift task (`QuickRecallModel.swift:30-70`).
5. Views compose menu, history, quick recall, settings, diagnostics, Markdown/link interactions, detail actions, and purge/storage flows.

**Seam:** Swift state lifecycle ↔ short-lived CLI processes ↔ SQLite revision state.

### 7. Setup and service management

- `setup` creates/prepares the DB, seeds the current clipboard, detects Homebrew/LaunchAgent conflicts, starts a provider, bumps revisions, and notifies the app (`src/cli/service/manage.rs`).
- Status examines launchctl/plist/process state, configured/running binaries, logs, DB freshness/policy/revision, and mismatch notes (`src/cli/service/status.rs`).
- Service actions support Homebrew services and a direct LaunchAgent (`src/cli/service/*`, `extras/launchd/*`).

**Seam:** service lifecycle and archive migration currently share ordinary DB open behavior.

### 8. Agent integration

- `agents context` aggregates service status, several stats windows, image/PDF counts, settings, app capabilities, and OCR status (`src/cli/commands/agents/context.rs`).
- OpenClaw and Hermes commands install/validate embedded skill packages (`agents/package.rs`, `openclaw_*`, `hermes_*`).
- Skill parity tests compare mirrored package content (`tests/skill_parity.rs`).

**Seam:** the command is an explicit, relatively expensive aggregate; it should remain on-demand, not become a polling surface.

## Load-bearing components

1. **`src/db/schema.sql` and `src/db/schema.rs`** — schema, triggers, FTS ownership, and migration gates.
2. **`src/model/builders.rs` + `text_projection.rs`** — payload identity and capture-time searchable meaning.
3. **`src/db/store/capture.rs`** — deduplication, event insertion, sensitive filtering, restore suppression.
4. **`src/db/read/search.rs` + generated SQL in `queries.rs`/`filter_sql.rs`** — user-visible retrieval correctness.
5. **`src/db/read/snapshot.rs`** — the current metadata/payload boundary, used by get/export/restore/projections.
6. **`src/cli/commands/runtime.rs`** — watcher orchestration and policy integration.
7. **`src/platform/macos.rs`** — live pasteboard consistency and exact restore.
8. **`src/db/store/ocr.rs` and `optimize.rs`** — expensive background mutation protocols.
9. **`AppModel`, `HistoryModel`, `CommandRunner`, `SnapshotDetailView`** — native app refresh and process lifecycle.

## Module coverage ledger

### Rust public/facade layer

- `src/main.rs`: process entry and exit handling.
- `src/lib.rs`: public module/export surface.
- `src/app.rs`: application-facing facade/tests.
- `src/archive.rs`: archive-facing types/facade/tests.
- `src/capture.rs`: capture builder facade/tests.
- `src/sensitive.rs`: API-key-like content detection used by policy.
- `src/file_url.rs`: file URL/path normalization.

### Rust model/platform/OCR

- `src/model/mod.rs`, `archive.rs`, `clipboard.rs`, `builders.rs`, `kinds.rs`, `text_projection.rs`, `builders/profile_tests.rs`: domain types, normalization, classification, fingerprinting, flattened text, and profiling tests.
- `src/platform/mod.rs`, `macos.rs`, `unsupported.rs`: platform abstraction, AppKit pasteboard implementation, and non-macOS errors.
- `src/ocr.rs`, `ocr/macos.rs`: OCR engine abstraction, orchestration, and Apple Vision implementation.

### Rust database

- `src/db.rs`, `core.rs`, `schema.rs`, `schema.sql`, `types.rs`, `impls.rs`, `sqlite_helpers.rs`: connection lifecycle, schema/migrations, shared types, and helpers.
- `src/db/read.rs`, `read/browse.rs`, `filter_sql.rs`, `queries.rs`, `query_analysis.rs`, `row_mapping.rs`, `search.rs`, `search_results.rs`, `snapshot.rs`, `stats.rs`, `value_decoding.rs`: browse/search/filter/detail/stat read paths.
- `src/db/store.rs`, `store/capture.rs`, `config.rs`, `ocr.rs`, `optimize.rs`, `purge.rs`, `rebuild.rs`, `revision.rs`, `settings.rs`: all persistent mutations, maintenance, cache rebuilds, revisions, and policy.
- `src/db/tests.rs`, `db/tests/*`, `db/read/tests.rs`, `db/store/tests.rs`: schema, migration, query-plan, performance-profile, round-trip, OCR, optimization, and trigger tests.

### Rust CLI

- `src/cli.rs`, `commands.rs`, `commands/entry.rs`: CLI root and dispatch.
- `schema.rs`, `parsing.rs`, `validate.rs`, `value_validation.rs`, `db_path.rs`: arguments, parsing, validation, and DB resolution.
- `errors.rs`, `exit.rs`, `runtime.rs`, `formats.rs`, `display.rs`, `presentation.rs`, `help.rs`: process/runtime and presentation policy.
- `output.rs`, `output/{json,markdown,model,row_text,support,text,toon,tests}.rs`: stable machine/text output models and renderers.
- `human.rs`, `human/{actions,detail,fmt,list,stats,theme}.rs`: human-oriented rendering.
- `commands/runtime.rs`, `retrieval.rs`, `retrieval/{cursor,recall}.rs`, `retrieval_support.rs`, `archive_mutate.rs`, `mutation_support.rs`, `settings.rs`, `storage.rs`, `ocr.rs`, `doctor.rs`, `notify.rs`, `app.rs`: user command adapters and orchestration.
- `service.rs`, `service/{context,launchctl,manage,model,render,status,tests}.rs`: watcher provider setup/status/control.
- `commands/agents.rs`, `agents/{context,doctor,hermes_manage,hermes_validate,openclaw_manage,openclaw_validate,package,support}.rs`: agent context and skill lifecycle.
- `cli/tests.rs` and command/service/output tests: parser, output, service, and command-unit coverage.

### Rust integration tests

- `tests/database_roundtrip.rs`: public archive round trip.
- `tests/cli_commands.rs` plus `tests/cli_commands/*`: end-to-end CLI behavior across retrieval, output, settings, service, app, agents, storage, and mutation.
- `tests/search_benchmark.rs`: seeded retrieval quality/latency harness.
- `tests/skill_parity.rs`: package mirror parity and references.

### Swift app

- `App/AppCommands.swift`, `ClipmemMenuBarApp.swift`: preference keys, scene/window activation, menu bar app composition.
- `Hotkey/HotKeyManager.swift`: global hotkey registration.
- `Logging/AppLoggers.swift`: OSLog categories.
- `Models/AppTypes.swift`, `ClipmemModels.swift`, `UpdateStatus.swift`: UI modes/filters, decoded CLI contracts, health/revision/storage/OCR models, update state.
- `Services/ClipmemClient/BinaryResolver.swift`, `macos/ClipmemMenuBar/ClipmemMenuBar/Services/ClipmemClient/ClipmemCommand.swift`, `ClipmemClient.swift`, `CommandRunner.swift`: CLI command construction, binary resolution, process execution/streaming, JSON/error mapping.
- `Services/LoginItemController.swift`, `UpdateChecker.swift`: login item and GitHub-release update behavior.
- `ViewModels/AppModel.swift`, `HistoryModel.swift`, `QuickRecallModel.swift`: global refresh/revision state, paged history/detail, debounced recall.
- `Utilities/ConfirmationAlertPresenter.swift`, `PasteboardActions.swift`: destructive confirmation and local pasteboard/link actions.
- `Views/DesignSystem.swift`, `SharedViews.swift`: style tokens and shared controls/banners/filter UI.
- `Views/MenuBarPanelView.swift`, `HistoryWindowView.swift`, `QuickRecallWindowView.swift`, `SettingsView.swift`, `DiagnosticsView.swift`, `ManualPurgeSheet.swift`: primary product surfaces.
- `Views/ResultRowView.swift`, `SnapshotDetailView.swift`, `ItemActionButtons.swift`: result/detail/actions/export.
- `Views/MarkdownTextRenderer.swift`, `CommandClickableMarkdownText.swift`: Markdown rendering, command-click hit testing, link opening, hover cursor.
- `Views/ScrollElasticityDisabler.swift`: AppKit scroll behavior bridge.
- `ClipmemMenuBarTests/*`: command construction, decoding, app/revision models, history/quick recall, process runner, pasteboard, updates, Markdown/link behavior, and launch-at-login defaults.

### Distribution, docs, and skills

- `.github/workflows/{ci,clawhub-skill,publish-crate,release}.yml`: Linux/macOS Rust checks, Xcode checks, packaging, Homebrew/cask/crate/skill publishing.
- `scripts/*`: dev app runner, file-length/version checks, ClawHub synchronization, LaunchAgent and skill install/uninstall.
- `extras/launchd/*`: launchd template.
- `skills/clipboard-memory/*`, `extras/agent-skills/*`, `extras/openclaw/*`, `extras/hermes/*`: canonical and mirrored skill instructions, schemas, examples, setup checks, and evals.
- `README.md`, `RELEASING.md`, `docs/*`: user, architecture, operations, contributor, privacy, and plan documentation.

## Main architectural seams to preserve explicitly

- **Payload identity vs observation history.** Do not collapse snapshots and events.
- **Source bytes vs derived text/search/preview.** Keep source immutable and derivations rebuildable.
- **Capture policy vs capture transport.** One policy service, multiple explicit capture modes.
- **Database migration vs normal operation.** Migrations are lifecycle work, not every-read work.
- **Metadata vs payload reads.** Make cost visible in API names and types.
- **Ranking vs confidence.** A database rank is not a calibrated confidence score.
- **Job discovery vs job ownership.** Selection alone is not a claim.
- **CLI contract vs native transport.** Preserve CLI automation even if the app later uses a local service protocol.
