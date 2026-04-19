#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_PATH="$ROOT_DIR/macos/ClipmemMenuBar/ClipmemMenuBar.xcodeproj"
DERIVED_DATA="$ROOT_DIR/macos/ClipmemMenuBar/DerivedData"
CLIPMEM_BIN="$ROOT_DIR/target/debug/clipmem"

echo "Building Rust backend..."
cargo build --manifest-path "$ROOT_DIR/Cargo.toml"

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
sleep 1
launchctl unsetenv CLIPMEM_BINARY_PATH

echo "ClipmemMenuBar launched with CLIPMEM_BINARY_PATH=$CLIPMEM_BIN"
