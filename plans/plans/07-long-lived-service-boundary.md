# Plan 7 — Long-lived local service boundary

**Priority:** P1/P2 architecture gate, not automatic  
**Primary owners:** service/runtime, database ownership, CLI transport, Swift client  
**Depends on:** plans 1–6 and roadmap measurement gate

## Why this is conditional

The app currently starts a CLI process per operation and polls revisions every two seconds, which is inefficient. But a daemon introduced before fixing schema-open and BLOB-read problems would merely centralize bad behavior while adding lifecycle/protocol complexity. Execute this plan only if post-P0 measurements satisfy the gate in `00-roadmap.md`.

## Required outcome if approved

One long-lived local coordinator owns watcher writes and background job execution, serves cancellable metadata/payload requests over a simple local protocol, and pushes revision events. The CLI remains a stable automation surface and has an explicit direct/read-only fallback policy.

## Scope

- Coordinator process/lifecycle and single-writer policy.
- Local framed protocol with version negotiation.
- Request/response, error, cancellation, event subscription, payload streaming.
- Swift transport adapter and CLI routing/fallback.
- Service setup/status/upgrade integration.
- Observability and compatibility tests.

Out of scope: remote network API, authentication/security design, cloud sync, multi-user service.

## Process model

Preferred evolution: the existing watcher service becomes the coordinator rather than adding a second daemon.

```text
clipmem service run / clipmemd
  ├── stable pasteboard watcher
  ├── one write coordinator / short write transactions
  ├── job claim worker pool
  ├── read request pool/connections
  ├── revision event publisher
  └── Unix domain socket endpoint
```

Do not hold one SQLite connection across unrelated concurrent tasks without a clear threading model. Use a small connection strategy:

- one serialized write coordinator or short-lived write/current connections;
- read-only connections per request/pool;
- migration only at service startup before accepting requests.

## Protocol

Use a length-prefixed JSON protocol or JSON-RPC-like envelope over a Unix domain socket. Avoid newline-delimited JSON for binary payload framing unless payloads are out-of-band files.

Envelope:

```text
Request { protocol_version, request_id, method, params, client_capabilities }
Response { request_id, ok, result | error }
Event { subscription_id, sequence, revision, kind, affected_ids? }
Cancel { request_id }
```

Requirements:

- protocol version negotiation and minimum supported version;
- stable typed error codes mapped to existing CLI exit categories;
- bounded request/response sizes;
- binary payload streaming as framed chunks or a service-created file handle/path with explicit lifetime; do not base64 large images in JSON;
- cancellation token reaches DB query/job scheduling;
- event sequence lets clients detect gaps and perform full revision refresh;
- per-request deadline.

Security is out of scope, but implementation still needs local path/lifecycle correctness; do not expand to TCP.

## API surface

Start narrow, based on measured app calls:

- `status_bundle` (service/settings/revision/recent summary if useful);
- `revision_subscribe`;
- `recent/search/timeline/recall`;
- `snapshot_metadata`;
- `preview_payload`;
- `restore`, `forget`, settings actions;
- maintenance operation start/status/cancel.

Do not expose raw SQL or mirror every internal function. CLI-only diagnostic/agent commands can remain direct subprocess logic if infrequent.

## CLI behavior

- CLI remains canonical user contract and output rendering.
- Add an internal client that can call coordinator for supported operations.
- Read-only commands may fall back to direct `open_read_only_current` if service unavailable and schema current.
- Mutating commands require an explicit ownership policy:
  - preferred: call coordinator when running;
  - if unavailable, direct mutation is allowed only after proving no coordinator writer owns the archive, or return service-unavailable guidance.
- `--direct` debug/repair mode may exist but must be explicit.
- Output JSON/human remains generated in CLI from typed response or shared models.

## Swift behavior

- Replace `ClipmemClient` implementation behind a protocol; views/view models do not know transport.
- One connection with reconnect/backoff.
- Subscribe to revisions rather than two-second process poll.
- On event gap/reconnect, fetch current revision and refresh selectively.
- Request cancellation is sent before local task completes.
- Preview payload streams directly to decoder/temp file and uses source/derivative version cache key.
- Retain subprocess fallback for one compatibility release, selected at startup with diagnostics.

## Lifecycle and upgrades

- Reuse Homebrew service/LaunchAgent management.
- Service startup runs explicit migration before socket readiness.
- Socket path includes archive/user identity as needed; stale socket cleanup is deterministic.
- Status reports protocol version, service binary/version, DB path/instance ID, worker state, and client compatibility.
- Binary upgrade: service exits/restarts cleanly; app reconnects and handles migration-required state.
- Only one coordinator per archive. Detect conflicts between Homebrew and LaunchAgent before binding/opening writer.

## Implementation sequence

1. Re-run measurements and write an architecture decision record approving/rejecting this plan.
2. Define shared transport-neutral service request/result traits/types from existing application services.
3. Specify protocol/version/error/cancellation/event contract with golden JSON/frame fixtures.
4. Implement coordinator in-process test harness over an in-memory channel before socket code.
5. Add Unix socket server/client and lifecycle readiness.
6. Move watcher loop and durable workers under coordinator using existing application services.
7. Implement narrow read/mutation endpoints and revision events.
8. Add CLI transport routing with direct read-only fallback.
9. Add Swift transport protocol/client, reconnect, cancellation, and event handling; keep subprocess fallback.
10. Update setup/status/doctor/logging.
11. Load/stress test concurrent search, preview, capture, OCR, and maintenance.
12. Remove high-frequency app polling only after event gap/reconnect tests pass.

## Edge cases

- Service dies mid-request: idempotent reads retry; mutations use operation/request IDs to query outcome before retry.
- Client reconnect misses events: sequence gap triggers current revision/full selective refresh.
- Two service providers start: only one archive lock/socket owner; loser exits with actionable conflict status.
- DB path override changes: close old connection/subscription, resolve new archive instance, ensure self-ignore, connect/start correct service or direct mode.
- Long preview stream cancellation cleans temp/resource state.
- Migration failure means socket is not advertised ready; status can still report failure through launch logs/direct probe.
- Old CLI/new service and new CLI/old service negotiate capabilities; unsupported methods fall back or error clearly.

## Tests and benchmarks

- Protocol golden fixtures and version compatibility matrix.
- Concurrent clients, cancellation, deadline, reconnect, sequence gap.
- Duplicate mutation request ID is idempotent.
- Provider conflict and stale socket recovery.
- Service crash/restart with leased jobs and WAL recovery.
- End-to-end Swift mock/real client tests.
- Compare app startup, recent refresh, detail preview P50/P95, processes spawned/minute, and energy/CPU before/after.

## Acceptance criteria

- Approved ADR demonstrates post-P0 need.
- Native app steady-state browsing/revision monitoring spawns no CLI subprocesses when service is healthy.
- CLI contracts and agent skills remain compatible.
- Read fallback never migrates/writes.
- Mutation ownership prevents concurrent uncoordinated writers.
- Events are gap-detectable and app recovers correctly.
- Measured latency/process/energy improvement justifies added complexity.

## Rollback

Keep subprocess/direct client for at least one release and a preference/diagnostic switch. Service schema/application services remain usable directly. If protocol proves unstable, app falls back without losing archive compatibility; do not couple source schema to service-only opaque state.
