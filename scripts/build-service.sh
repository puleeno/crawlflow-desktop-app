#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SRC_TAURI="$PROJECT_DIR/src-tauri"

echo "[build-service] Compiling crawlflow-service..."
cargo build --manifest-path "$SRC_TAURI/Cargo.toml" --bin crawlflow-service --release

# Determine target triple
TARGET="${TARGET:-"$(rustc -vV | grep host | cut -d' ' -f2)"}"

mkdir -p "$SRC_TAURI/binaries"

cp "$SRC_TAURI/target/release/crawlflow-service" \
   "$SRC_TAURI/binaries/crawlflow-service-${TARGET}"

echo "[build-service] Done: binaries/crawlflow-service-${TARGET}"
