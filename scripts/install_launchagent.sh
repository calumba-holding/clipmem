#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

INSTALL_ROOT="${CLIPMEM_INSTALL_ROOT:-$HOME/.local}"
DB_PATH="${CLIPMEM_DB_PATH:-$HOME/Library/Application Support/clipmem/clipmem.sqlite3}"
INTERVAL_MS="${CLIPMEM_INTERVAL_MS:-350}"

BIN_DIR="${INSTALL_ROOT}/bin"
BIN_PATH="${BIN_DIR}/clipmem"
APP_SUPPORT_DIR="$(dirname -- "${DB_PATH}")"
LOG_DIR="${APP_SUPPORT_DIR}/logs"
PLIST_PATH="$HOME/Library/LaunchAgents/io.openclaw.clipmem.watch.plist"
TEMPLATE_PATH="${PROJECT_DIR}/extras/launchd/io.openclaw.clipmem.watch.plist.template"

mkdir -p "$BIN_DIR" "$APP_SUPPORT_DIR" "$LOG_DIR" "$HOME/Library/LaunchAgents"

cargo install --path "$PROJECT_DIR" --root "$INSTALL_ROOT"

STDOUT_PATH="${LOG_DIR}/clipmem.stdout.log"
STDERR_PATH="${LOG_DIR}/clipmem.stderr.log"

python3 - "$TEMPLATE_PATH" "$PLIST_PATH" "$BIN_PATH" "$DB_PATH" "$INTERVAL_MS" "$PROJECT_DIR" "$STDOUT_PATH" "$STDERR_PATH" <<'PY'
import pathlib
import sys

template_path = pathlib.Path(sys.argv[1])
out_path = pathlib.Path(sys.argv[2])

text = template_path.read_text()
replacements = {
    "{{CLIPMEM_BIN}}": sys.argv[3],
    "{{CLIPMEM_DB_PATH}}": sys.argv[4],
    "{{CLIPMEM_INTERVAL_MS}}": sys.argv[5],
    "{{WORKING_DIRECTORY}}": sys.argv[6],
    "{{STDOUT_PATH}}": sys.argv[7],
    "{{STDERR_PATH}}": sys.argv[8],
}

for key, value in replacements.items():
    text = text.replace(key, value)

out_path.write_text(text)
PY

LABEL="io.openclaw.clipmem.watch"

launchctl bootout "gui/${UID}/${LABEL}" >/dev/null 2>&1 || true
launchctl bootstrap "gui/${UID}" "$PLIST_PATH"
launchctl enable "gui/${UID}/${LABEL}"
launchctl kickstart -k "gui/${UID}/${LABEL}"

echo "Installed ${LABEL}"
echo "Binary: ${BIN_PATH}"
echo "Database: ${DB_PATH}"
echo "Plist: ${PLIST_PATH}"
