#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

CLIPMEM_BIN="$(command -v clipmem || true)"
if [[ -z "$CLIPMEM_BIN" ]]; then
  echo "clipmem is not on PATH for this process."
  echo "Install it first or add its bin directory to the PATH seen by OpenClaw."
  exit 1
fi

SRC_DIR="${PROJECT_DIR}/extras/openclaw/clipboard_memory"
DEST_DIR="${OPENCLAW_SKILLS_DIR:-$HOME/.openclaw/skills}/clipboard_memory"

mkdir -p "$(dirname -- "$DEST_DIR")"
rm -rf "$DEST_DIR"
cp -R "$SRC_DIR" "$DEST_DIR"

echo "Installed OpenClaw skill into ${DEST_DIR}"
echo "Resolved clipmem binary: ${CLIPMEM_BIN}"
echo "Reload OpenClaw skills or restart OpenClaw if the new skill does not appear immediately."
