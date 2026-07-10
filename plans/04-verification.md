# Verification record

## Environment and constraints

- Source version: 0.5.6 (`Cargo.toml` and Xcode project agree).
- Source archive contained no `.git` metadata.
- Rust toolchain (`cargo`, `rustc`) was unavailable in the execution environment, so Rust format, Clippy, and test commands could not be executed.
- Swift 6.2.1 was present on Linux, but AppKit/Xcode were unavailable, so the native app could not be built or tested.
- SQLite 3.46.1 with FTS5/trigram support was available through Python.
- The external `clawhub` binary was unavailable.

These constraints are not hidden: all conclusions marked confirmed are grounded in code paths and/or executable SQL behavior. Compiler/test regressions still require verification in the repository's normal CI environment.

## Checks run

| Check | Result | Artifact |
|---|---|---|
| Version synchronization | Passed: Cargo and menu app 0.5.6 | [`evidence/version-sync-2.txt`](evidence/version-sync-2.txt) |
| File-length lint | Passed after copying into a temporary initialized Git repo (the archive lacked `.git`) | [`evidence/file-length-check.txt`](evidence/file-length-check.txt) |
| `cargo fmt --check` | Not run: `cargo` missing | [`evidence/cargo-fmt.txt`](evidence/cargo-fmt.txt) |
| `cargo test --all-targets` | Not run: `cargo` missing | [`evidence/cargo-test.txt`](evidence/cargo-test.txt) |
| `cargo clippy --all-targets -- -D warnings` | Not run: `cargo` missing | [`evidence/cargo-clippy.txt`](evidence/cargo-clippy.txt) |
| ClawHub sync check | Blocked: `clawhub` missing | [`evidence/clawhub-sync.txt`](evidence/clawhub-sync.txt) |
| Static inventories | Completed for all Rust/Swift/text files | workspace inventory files |
| SQL schema reapplication | Passed in SQLite 3.46.1 | [`evidence/sqlite-invariant-probes.json`](evidence/sqlite-invariant-probes.json) |
| FTS5 BM25 behavior | Distinct negative ranks observed | [`evidence/fts-bm25-probe.txt`](evidence/fts-bm25-probe.txt) |

## Executable database probes

The probe script copied the real `src/db/schema.sql` into an in-memory SQLite database and exercised trigger/filter/integrity behavior without reimplementing the schema.

### Probe outcomes

1. **Schema reapply:** applying the full idempotent schema twice succeeded.
2. **Text-presence semantics:** an image-only snapshot with preview `[image · 12 bytes]` satisfied the current has-text expression.
3. **Missing composite FK:** an `item_representations` row for item index 99 was accepted; `PRAGMA foreign_key_check` returned no rows; the normal item-loader join shape did not see it.
4. **Historical app mismatch:** event filter cache contained both “first app” and “second app,” while literal haystack contained only “second app.” App filtering matched the first app, literal free-text could not.
5. **Restore suppression:** first matching event insert was ignored and deleted the marker; second matching insert succeeded.
6. **Cache/FTS population:** snapshot stats, projection, event filter, literal cache, native FTS, and literal FTS rows populated.

Raw results are in [`evidence/sqlite-invariant-probes.json`](evidence/sqlite-invariant-probes.json); the executable script is [`evidence/sqlite_invariant_probes.py`](evidence/sqlite_invariant_probes.py).

### BM25 probe

For rows containing different frequencies/context of `git`, FTS5 returned ranks such as:

```text
-1.4193548387096774e-06
-1.1139240506329113e-06
```

The current `max(0.0)` normalization maps both to `1.0`, directly validating F-01.

## Existing test coverage that is useful

The repository already has substantial tests for:

- content deduplication and round trips;
- schema migrations through version 18;
- trigger-maintained caches and indexes;
- FTS/literal query-mode selection and escaping;
- recent/timeline cursor ordering;
- shared filter application;
- image optimization pixel equality and idempotent status;
- OCR cache/index behavior;
- CLI parsing, exit codes, output envelopes/formats;
- service setup/status and agent skill parity;
- Swift command construction/decoding, revision refresh behavior, history/quick recall state, Markdown links, and process output draining.

This is a good base. The problem is that several load-bearing contracts are not represented.

## Required missing tests before/with the plans

### Retrieval

- FTS raw-rank ordering and nonconstant user score.
- Weak-match branch executes for deliberately weak FTS hit.
- Native + OCR duplicate merges evidence.
- Multi-page mixed-source search has no gaps/duplicates for adversarial score distributions.
- Cursor is stable across ties and source combinations.
- `--has-text` excludes placeholders and includes ready OCR-only snapshots.
- Historical app search/filter contract.

### Capture/restore

- Two concurrent store attempts for one restore generation are both suppressed.
- Stable pasteboard reader retries changed generation and never stores mixed capture.
- Watch/manual/setup mode policy matrix.
- Restore preparation failure leaves current pasteboard untouched.
- Write failure triggers best-effort rollback and structured result.
- Actual frontmost app remains available when content origin is inferred.

### Database/open/read paths

- Read-only current open performs no write and succeeds on read-only filesystem/file permissions.
- Current-schema open does not execute schema DDL or acquire immediate transaction.
- Migration-required typed error from read-only/current open.
- Metadata/get/list queries do not select BLOB columns (trace/profile contract).
- Export reads exactly one payload and atomically replaces destination.
- Composite FK rejects orphan representation; migration handles seeded orphan fixture.

### Jobs/optimizer

- Two workers atomically claim different jobs.
- Lease expiry/retry and stale-owner completion rejection.
- Cancellation stops new claims and leaves durable resumable state.
- Source bytes/fingerprint unchanged after derivative generation.
- Exact byte-for-byte export/restore after any reversible storage encoding.
- Bounded memory/concurrency behavior with large fixture images.
- Every visible status transition bumps declared revisions.

### Native app

- Stale rows remain after transient refresh error.
- External revision reloads selected detail when content version changes.
- History query cancellation terminates the underlying process.
- Timeout is distinguishable from user cancellation and escalates termination.
- Preview reload key includes representation/content version.
- Self-ignore is installed for each archive identity.

## Pressure tests applied to recommendations

### Why not remove all triggers immediately?

Triggers currently provide strong local consistency for many direct SQL writes and tests. Removing them before application services and canonical document backfill exist would increase risk. The plan retains narrow integrity/index triggers during dual-write and removes broad rebuild triggers only after shadow verification.

### Why not build a daemon first?

A daemon would hide some process startup and serialize writes, but it would not fix wrong score semantics, BLOB-heavy APIs, source mutation, or missing FKs. It would also create a protocol/lifecycle migration before the true cost is measured. The roadmap therefore treats it as a gate after P0 work.

### Why preserve the snapshot/event model?

It expresses the product well, supports dedupe and history independently, and is not the source of the observed complexity. Replacing it would add migration risk without solving the documented defects.

### Why not simply negate BM25 and keep a confidence number?

Negating preserves ordering but still does not calibrate across query length, corpus, field weights, native/OCR sources, or time. The plan separates ranking from confidence and requires an evaluated contract.

### Why not keep pixel-equivalent image replacement?

The product explicitly values exact rich restore and raw export. Pixel equality is a different contract and does not preserve container metadata or stable source fingerprint. A user-selectable destructive transcode could exist later, but it must not masquerade as transparent archive optimization.

## Verification required after implementation

Each plan contains acceptance tests. At release gate, run at minimum:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
python3 scripts/check_file_lengths.py
python3 scripts/check_version_sync.py
xcodebuild test ... (the CI project/scheme)
python3 scripts/clawhub_skill_sync.py check  # where clawhub credentials/tooling exist
```

Also run the search quality/latency benchmark with before/after saved reports and a new mixed native/OCR pagination corpus. Do not accept a retrieval rewrite based only on unit tests.
