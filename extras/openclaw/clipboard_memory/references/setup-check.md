# Clipboard Memory — Setup Check

Prose mirror of `scripts/check-setup.sh`. Use this when your runtime can't execute shell scripts directly. Byte-identical across skill packages.

Run these commands in order. Stop at the first failure and repair before querying.

## 1. Binary present

```bash
clipmem --version
```

Expect a version line on stdout, exit code 0. If not, `clipmem` is missing from PATH. Install via `brew install tristanmanchester/tap/clipmem` or `cargo install clipmem`.

## 2. Database healthy

```bash
clipmem doctor --json
```

Expect exit 0 and `"fts5_create_virtual_table_ok": true` in the JSON payload. Non-zero exit means the SQLite archive is corrupt or inaccessible (failure is signalled via exit code, not a JSON `errors` field). `fts5_create_virtual_table_ok: false` means FTS queries will fail — either use `--mode literal` or rebuild the database.

## 3. Watcher freshness

```bash
clipmem timeline --hours 1 --format json --limit 1
```

Expect `results` to contain at least one row if the user has copied anything in the last hour. Zero rows is a warning, not a failure — it may simply mean the user hasn't copied recently.

## 4. LaunchAgent loaded (macOS)

```bash
launchctl list | grep io.openclaw.clipmem.watch
```

Expect one line with a PID column > 0. If missing or PID is `-`, the watcher is not running:

```bash
./scripts/install_launchagent.sh   # from the clipmem repo
# or
clipmem watch --skip-initial &     # foreground fallback
```

## 5. OpenClaw integration (optional)

If the agent is OpenClaw, also check:

```bash
clipmem agents openclaw doctor
```

Expect every check to report `[OK]`. `[FAIL]` lines include remediation steps.

## Interpretation

| Symptom | Likely cause |
|---|---|
| `clipmem` not found | binary not installed or not on PATH |
| `doctor` exits non-zero | database lock, corruption, or permission issue |
| `timeline --hours 1` empty + no LaunchAgent | watcher not running |
| FTS query errors | `fts5_create_virtual_table_ok: false` — switch to `--mode literal` |
| Sandboxed agent can't see the archive | PATH or file-access scope; rerun `openclaw sandbox explain` |

See `scripts/check-setup.sh` for the executable version with categorised exit codes (0 healthy, 1 watcher stale, 2 binary missing, 3 doctor failed).
