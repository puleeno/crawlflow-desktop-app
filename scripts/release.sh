#!/usr/bin/env bash
set -euo pipefail

# ── CrawlFlow release build ──────────────────────────────────────────
# Builds the service + desktop app for the current platform.
#
# Usage:
#   ./scripts/release.sh                  # Build for current host
#   TARGET=aarch64-apple-darwin ./scripts/release.sh   # Cross-compile
#
# To build for all platforms, run on each platform (or use CI):
#   macOS ARM:   TARGET=aarch64-apple-darwin        ./scripts/release.sh
#   macOS Intel: TARGET=x86_64-apple-darwin         ./scripts/release.sh
#   Linux:       TARGET=x86_64-unknown-linux-gnu    ./scripts/release.sh
#   Windows:     TARGET=x86_64-pc-windows-msvc      ./scripts/release.sh
# ───────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "═══════════════════════════════════════════════════════════════"
echo "  CrawlFlow Release Build"
echo "  Target: ${TARGET:-"(host)"}"
echo "═══════════════════════════════════════════════════════════════"

# Step 1: Build the background service
echo ""
echo "── Step 1/2: Building crawlflow-service ──"
bash "$SCRIPT_DIR/build-service.sh"

# Step 2: Build the Tauri desktop app (includes frontend + bundling)
echo ""
echo "── Step 2/2: Building Tauri desktop app ──"
cd "$PROJECT_DIR"
npm run tauri build

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Release build complete!"
echo "  Bundle location: src-tauri/target/release/bundle/"
echo "═══════════════════════════════════════════════════════════════"
