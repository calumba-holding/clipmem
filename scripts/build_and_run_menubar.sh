#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_PATH="$ROOT_DIR/macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj"
DERIVED_DATA="$ROOT_DIR/macos/ClipmemMenuBar/DerivedData"
CLIPMEM_BIN="$ROOT_DIR/target/debug/clipmem"
APP_NAME="ClipmemMenuBar"

stop_existing_app() {
  local pids

  pids="$(pgrep -x "$APP_NAME" || true)"
  if [[ -z "$pids" ]]; then
    return
  fi

  echo "Stopping existing $APP_NAME instances..."
  osascript -e 'tell application id "io.openclaw.clipmem.menubar" to quit' >/dev/null 2>&1 || true

  local deadline=$((SECONDS + 5))
  while [[ $SECONDS -lt $deadline ]]; do
    if ! pgrep -x "$APP_NAME" >/dev/null; then
      return
    fi
    sleep 0.2
  done

  pkill -x "$APP_NAME" || true
  sleep 0.5
}

running_app_paths() {
  local pids

  pids="$(pgrep -x "$APP_NAME" | paste -sd, - || true)"
  if [[ -z "$pids" ]]; then
    return
  fi

  ps -ww -p "$pids" -o command= | sed 's#/Contents/MacOS/ClipmemMenuBar$##'
}

echo "Building Rust backend..."
cargo build --manifest-path "$ROOT_DIR/Cargo.toml"

stop_existing_app

echo "Building macOS menu bar app..."
xcodebuild \
  -project "$PROJECT_PATH" \
  -scheme ClipmemMenuBar \
  -configuration Debug \
  -derivedDataPath "$DERIVED_DATA" \
  build

APP_PATH="$DERIVED_DATA/Build/Products/Debug/ClipmemMenuBar.app"
if [[ ! -d "$APP_PATH" ]]; then
  echo "Built app was not found at $APP_PATH" >&2
  exit 1
fi

echo "Launching $APP_PATH"
launchctl setenv CLIPMEM_BINARY_PATH "$CLIPMEM_BIN"
open -n "$APP_PATH"
sleep 2
launchctl unsetenv CLIPMEM_BINARY_PATH

if ! running_app_paths | grep -Fxq "$APP_PATH"; then
  echo "Failed to verify $APP_PATH is running." >&2
  exit 1
fi

echo "$APP_NAME launched with CLIPMEM_BINARY_PATH=$CLIPMEM_BIN"
