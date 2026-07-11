# Plan 07 long-lived service gate measurements

The July 10, 2026 measurements do not justify replacing direct CLI reads with a
long-lived local service. Warm subprocess latency is well below the gate, and
targeted history, detail, and search reads are below a 16.7 ms frame budget at
P95. Retain the direct CLI architecture and pursue a narrower revision event or
batching mechanism only if two-second polling creates observable energy impact.

## Test setup

The measurements used the following setup:

- Branch: `traycer/plan-07-profiling`, after plans 1–4.
- Build: `cargo build --release`.
- Binary: `target/release/clipmem` version 0.5.6.
- Host: macOS, measured locally with a monotonic clock.
- Archive: 5,046 snapshots in a 23 MB SQLite database.
- Fixture composition: 5,000 benchmark filler snapshots and 46 named realistic
  text, rich-content, image, and OCR cases from `tests/search_benchmark.rs`.
- Sampling: 10 unmeasured warmups followed by 100 measured iterations for each
  command.
- Timing boundary: immediately before process spawn through successful process
  exit, with standard output and standard error redirected to `/dev/null`.
- Percentiles: median is the mean of sorted samples 50 and 51; P95 is sorted
  sample 95.

`hyperfine` was not installed, so a Ruby loop used
`Process.clock_gettime(Process::CLOCK_MONOTONIC)` and `system` to preserve the
same spawn-to-exit boundary paid by the menu bar app.

## Results

All commands used the release binary and the global `--db` option. JSON output
was generated inside the measured process even though it was redirected after
generation.

| Operation | Measured command | Median | P95 | Approximate median above spawn baseline |
| --- | --- | ---: | ---: | ---: |
| Spawn baseline | `clipmem --version` | 5.018 ms | 8.256 ms | — |
| Revision | `clipmem --db DB service revision --format json` | 6.430 ms | 8.355 ms | 1.412 ms |
| Recent history | `clipmem --db DB recent --limit 50 --format json` | 9.605 ms | 10.403 ms | 4.587 ms |
| Snapshot detail | `clipmem --db DB get 5040 --events 10 --format json` | 5.736 ms | 6.115 ms | 0.718 ms |
| Search | `clipmem --db DB search --limit 10 --format json -- 'browser archive examples'` | 6.904 ms | 7.703 ms | 1.886 ms |

The subtraction in the last column is diagnostic, not a rigorous decomposition:
`--version` does less initialization than a database command. It nonetheless
shows that process startup dominates these warm paths and that the remaining
database and JSON work is small.

The direct database search harness also reported a 0.644 ms median and 0.715 ms
P95 for a plain automatic-mode search. The corresponding end-to-end CLI search
above remained 7.703 ms at P95, including spawn, database open, query, JSON
serialization, and exit.

## Gate decision

The recommendation is to reject Plan 07 for now. The current evidence favors
the simpler direct CLI/subprocess boundary.

| Gate criterion | Result | Recommendation |
| --- | --- | --- |
| Median warm `service revision` exceeds 25 ms, or visible energy/process churn exists | **No for latency; partial for churn.** The measured median is 6.430 ms, 74% below the 25 ms threshold. Two-second polling still creates 30 short-lived processes per minute by design, but this run did not capture an energy regression. | Do not approve a service from latency data. If polling remains a concern, measure Energy Log or `powermetrics` during an app-level idle session and first test a narrow event/batching solution. |
| History/detail P95 exceeds the UI target despite targeted reads | **No.** Recent history is 10.403 ms P95, detail is 6.115 ms P95, and search is 7.703 ms P95. Each is below a 16.7 ms frame budget before Swift-side rendering. | Retain targeted direct reads. Profile Swift rendering separately if the UI remains visibly slow. |
| Write contention or job ownership is materially simpler with one coordinator | **Not demonstrated.** This benchmark found no write contention and provides no evidence that coordinator ownership would remove a current failure mode. | Do not add a coordinator for hypothetical simplification. Reopen the gate only with reproducible contention, lease, or ownership failures. |
| Launch/lifecycle complexity is acceptable on supported macOS versions | **Not established.** Plan 07 adds socket readiness, stale-socket cleanup, version negotiation, reconnect behavior, duplicate-provider exclusion, upgrade handling, and fallback policy. None of that complexity is necessary to meet the measured read targets. | Treat lifecycle cost as unacceptable relative to the measured benefit. Keep the existing service management surface and direct read fallback. |

## Decision and reopen conditions

Keep the direct CLI architecture. Do not implement the long-lived coordinator
described by Plan 07 based on these measurements.

Reopen the decision only when at least one of these conditions has measured
evidence:

- warm revision median rises above 25 ms on a supported baseline Mac;
- app-level idle profiling shows material energy impact from revision polling;
- history, detail, or search P95 misses the agreed UI target after targeted
  reads;
- a reproducible write-contention or ownership failure is materially resolved
  by single-coordinator semantics.

