#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

SRC_DIR="${PROJECT_DIR}/extras/openclaw/clipboard-memory"
DEST_DIR="${OPENCLAW_SKILLS_DIR:-$HOME/.openclaw/skills}/clipboard-memory"

mkdir -p "$(dirname -- "$DEST_DIR")"
rm -rf "$DEST_DIR"
cp -R "$SRC_DIR" "$DEST_DIR"

echo "Installed OpenClaw skill into ${DEST_DIR}"
