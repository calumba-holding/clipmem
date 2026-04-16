#!/usr/bin/env bash
set -euo pipefail

LABEL="io.openclaw.clipmem.watch"
PLIST_PATH="$HOME/Library/LaunchAgents/${LABEL}.plist"

launchctl bootout "gui/${UID}/${LABEL}" >/dev/null 2>&1 || true
launchctl disable "gui/${UID}/${LABEL}" >/dev/null 2>&1 || true

rm -f "$PLIST_PATH"

echo "Removed ${LABEL}"
