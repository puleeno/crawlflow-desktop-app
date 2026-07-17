#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SRC_TAURI="$PROJECT_DIR/src-tauri"

# Allow overriding TARGET (useful for cross-compilation in CI)
# Prefer Tauri's own target triple env var (set during `tauri build`), then TARGET, then host detection
if [ -n "${TAURI_ENV_TARGET_TRIPLE:-}" ]; then
  DEFAULT_TARGET="$TAURI_ENV_TARGET_TRIPLE"
else
  DEFAULT_TARGET="$(rustc -vV | grep host | cut -d' ' -f2)"
fi
TARGET="${TARGET:-$DEFAULT_TARGET}"

# Determine binary extension per platform
case "$TARGET" in
  *-pc-windows-*)
    BIN_EXT=".exe"
    ;;
  *)
    BIN_EXT=""
    ;;
esac

# Kill any running crawlflow-service / desktop app that would hold the binary
# open and prevent cargo from overwriting it (stale-binary bug).
echo "[build-service] Stopping any running crawlflow-service / app processes..."
if [[ "$(uname)" == "Darwin" || "$(uname)" == "Linux" ]]; then
  # Unload the macOS launch agent if present so it does not respawn the service
  if [[ "$(uname)" == "Darwin" ]]; then
    PLIST="$HOME/Library/LaunchAgents/com.CrawlFlow.desktop-service.plist"
    if [ -f "$PLIST" ]; then
      launchctl bootout "gui/$(id -u)/com.CrawlFlow.desktop-service" 2>/dev/null || \
        launchctl unload "$PLIST" 2>/dev/null || true
    fi
  fi
  pkill -f "target/debug/crawlflow-service" 2>/dev/null || true
  pkill -f "target/release/crawlflow-service" 2>/dev/null || true
  pkill -f "binaries/crawlflow-service" 2>/dev/null || true
  pkill -f "target/debug/crawlflow\b" 2>/dev/null || true
  pkill -f "target/release/crawlflow\b" 2>/dev/null || true
  # Give the OS a moment to release the file handles
  sleep 1
fi

echo "[build-service] Compiling crawlflow-service for target: ${TARGET}"

# If a different target than host, pass --target for cross-compilation
if [ "$TARGET" != "$(rustc -vV | grep host | cut -d' ' -f2)" ]; then
  cargo build --manifest-path "$SRC_TAURI/Cargo.toml" \
    --bin crawlflow-service --release --target "$TARGET"
  SRC_BIN="$SRC_TAURI/target/$TARGET/release/crawlflow-service${BIN_EXT}"
else
  cargo build --manifest-path "$SRC_TAURI/Cargo.toml" \
    --bin crawlflow-service --release
  SRC_BIN="$SRC_TAURI/target/release/crawlflow-service${BIN_EXT}"
fi

mkdir -p "$SRC_TAURI/binaries"

# Clean up stale binaries for other targets — keep only the current one
echo "[build-service] Cleaning stale service binaries..."
rm -f "$SRC_TAURI/binaries"/crawlflow-service-*

DEST_BIN="$SRC_TAURI/binaries/crawlflow-service-${TARGET}${BIN_EXT}"
cp "$SRC_BIN" "$DEST_BIN"
echo "[build-service] Copied to: $DEST_BIN"
echo "[build-service] Done."
