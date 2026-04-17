#!/bin/sh
# clipmem skill — setup health check
#
# Verifies that clipmem is installed, its database is healthy, the watcher
# daemon has recent captures, and (optionally) the OpenClaw integration is
# wired up.
#
# Exit codes:
#   0  all checks passed (green) or watcher freshness is the only soft warning
#   1  watcher stale: clipmem is installed and healthy but nothing captured
#      recently and no background service is running
#   2  binary missing: clipmem is not on PATH
#   3  doctor failed: clipmem doctor reported a hard error
#
# Intended for agents; stdout is human-readable, stderr carries diagnostics.

set -u

red()    { printf '\033[31m%s\033[0m\n' "$*"; }
yellow() { printf '\033[33m%s\033[0m\n' "$*"; }
green()  { printf '\033[32m%s\033[0m\n' "$*"; }

fail() {
    red "FAIL: $1"
    exit "$2"
}

# ---------------------------------------------------------------------------
# 1. clipmem binary
# ---------------------------------------------------------------------------
if ! command -v clipmem >/dev/null 2>&1; then
    fail "clipmem is not on PATH. Install via 'brew install tristanmanchester/tap/clipmem' or 'cargo install clipmem'." 2
fi

VERSION=$(clipmem --version 2>/dev/null || true)
green "OK: ${VERSION:-clipmem present}"

# ---------------------------------------------------------------------------
# 2. clipmem doctor
# ---------------------------------------------------------------------------
DOCTOR_OUT=$(clipmem doctor --json 2>&1)
DOCTOR_STATUS=$?

if [ "$DOCTOR_STATUS" -ne 0 ]; then
    red "FAIL: clipmem doctor exited ${DOCTOR_STATUS}"
    printf '%s\n' "$DOCTOR_OUT" >&2
    exit 3
fi

green "OK: clipmem doctor exited cleanly"

if printf '%s' "$DOCTOR_OUT" | grep -Eq '"fts5_create_virtual_table_ok"[[:space:]]*:[[:space:]]*true'; then
    green "OK: FTS5 available"
else
    yellow "WARN: FTS5 not available; --mode fts will fail. Use --mode literal."
fi

# ---------------------------------------------------------------------------
# 3. clipmem service status
# ---------------------------------------------------------------------------
STATUS_OUT=$(clipmem service status --json 2>&1)
STATUS_CODE=$?
if [ "$STATUS_CODE" -ne 0 ]; then
    red "FAIL: clipmem service status exited ${STATUS_CODE}"
    printf '%s\n' "$STATUS_OUT" >&2
    exit 3
fi

STATUS_VARS=$(
    printf '%s' "$STATUS_OUT" | python3 -c "import json,sys; data=json.load(sys.stdin); print('homebrew_running=%d' % (1 if data['homebrew']['running'] else 0)); print('homebrew_loaded=%d' % (1 if data['homebrew']['loaded'] else 0)); print('launchagent_running=%d' % (1 if data['launchagent']['running'] else 0)); print('launchagent_loaded=%d' % (1 if data['launchagent']['loaded'] else 0)); print('stale=%d' % (1 if data['stale'] else 0)); fresh=data.get('recent_capture_within_last_hour'); print('recent_capture_within_last_hour=%s' % ('-1' if fresh is None else ('1' if fresh else '0'))); print('conflict=%d' % (1 if data.get('conflict') else 0))"
) || {
    red "FAIL: could not parse clipmem service status JSON"
    printf '%s\n' "$STATUS_OUT" >&2
    exit 3
}

eval "$STATUS_VARS"

if [ "${homebrew_running}" -eq 1 ]; then
    green "OK: Homebrew service homebrew.mxcl.clipmem is running"
elif [ "${homebrew_loaded}" -eq 1 ]; then
    yellow "WARN: Homebrew service homebrew.mxcl.clipmem is loaded but not running"
fi

if [ "${launchagent_running}" -eq 1 ]; then
    green "OK: LaunchAgent io.openclaw.clipmem.watch is running"
elif [ "${launchagent_loaded}" -eq 1 ]; then
    yellow "WARN: LaunchAgent io.openclaw.clipmem.watch is loaded but not running"
fi

if [ "${homebrew_running}" -eq 0 ] && [ "${homebrew_loaded}" -eq 0 ] \
    && [ "${launchagent_running}" -eq 0 ] && [ "${launchagent_loaded}" -eq 0 ]; then
    yellow "WARN: no clipmem background service is loaded"
    yellow "      Run: clipmem setup"
    yellow "      Or:  brew services start clipmem"
fi

if [ "${recent_capture_within_last_hour}" -eq 1 ]; then
    green "OK: clipboard capture observed in the last hour"
elif [ "${recent_capture_within_last_hour}" -eq 0 ]; then
    yellow "WARN: no clipboard captures in the last hour"
fi

if [ "${conflict}" -eq 1 ]; then
    yellow "WARN: both Homebrew and direct LaunchAgent services are installed"
    yellow "      Remove one with: brew services stop clipmem"
    yellow "      Or:              clipmem service uninstall"
fi

# ---------------------------------------------------------------------------
# 4. OpenClaw integration (optional, best-effort)
# ---------------------------------------------------------------------------
if clipmem agents openclaw --help >/dev/null 2>&1; then
    if clipmem agents openclaw doctor >/dev/null 2>&1; then
        green "OK: clipmem agents openclaw doctor passed"
    else
        yellow "WARN: clipmem agents openclaw doctor reported issues; run it directly for details"
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
if [ "${stale}" -eq 1 ]; then
    red "STALE: no recent captures and no background watcher is running. Run 'clipmem setup' or 'brew services start clipmem' and retry."
    exit 1
fi

green "All checks passed."
exit 0
