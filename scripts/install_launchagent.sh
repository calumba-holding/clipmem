#!/usr/bin/env bash
set -euo pipefail
umask 077

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

install -d -m 700 "$BIN_DIR" "$APP_SUPPORT_DIR" "$LOG_DIR" "$HOME/Library/LaunchAgents"

cargo install --path "$PROJECT_DIR" --root "$INSTALL_ROOT" --force --locked

STDOUT_PATH="${LOG_DIR}/clipmem.stdout.log"
STDERR_PATH="${LOG_DIR}/clipmem.stderr.log"
touch "$STDOUT_PATH" "$STDERR_PATH"
chmod 600 "$STDOUT_PATH" "$STDERR_PATH"

cat >"$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>io.openclaw.clipmem.watch</string>

    <key>ProgramArguments</key>
    <array>
      <string>${BIN_PATH}</string>
      <string>watch</string>
      <string>--skip-initial</string>
      <string>--db</string>
      <string>${DB_PATH}</string>
      <string>--interval-ms</string>
      <string>${INTERVAL_MS}</string>
    </array>

    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>${STDOUT_PATH}</string>
    <key>StandardErrorPath</key>
    <string>${STDERR_PATH}</string>
  </dict>
</plist>
EOF

chmod 600 "$PLIST_PATH"

LABEL="io.openclaw.clipmem.watch"

launchctl bootout "gui/${UID}/${LABEL}" >/dev/null 2>&1 || true
launchctl bootstrap "gui/${UID}" "$PLIST_PATH"
launchctl enable "gui/${UID}/${LABEL}"
launchctl kickstart -k "gui/${UID}/${LABEL}"

echo "Installed ${LABEL}"
echo "Binary: ${BIN_PATH}"
echo "Database: ${DB_PATH}"
echo "Plist: ${PLIST_PATH}"
