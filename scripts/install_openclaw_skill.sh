#!/usr/bin/env bash
set -euo pipefail

CLIPMEM_BIN="$(command -v clipmem || true)"
if [[ -z "$CLIPMEM_BIN" ]]; then
  echo "clipmem is not on PATH for this process."
  echo "Install it first or add its bin directory to the PATH seen by OpenClaw."
  exit 1
fi

if [[ -n "${OPENCLAW_SKILLS_DIR:-}" ]]; then
  exec "$CLIPMEM_BIN" agents openclaw install-skill --dest "${OPENCLAW_SKILLS_DIR}/clipboard_memory" --force "$@"
fi

exec "$CLIPMEM_BIN" agents openclaw install-skill --shared --force "$@"
