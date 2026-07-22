#!/usr/bin/env bash
set -e

TARGET=$(rustc -vV | grep host | cut -d' ' -f2)
PROFILE="${CARGO_PROFILE:-debug}"
if [ "$PROFILE" = "dev" ] || [ -z "$PROFILE" ]; then
  PROFILE="debug"
  BUILD_DIR="target/debug"
else
  BUILD_DIR="target/$PROFILE"
fi

SIDECAR="src-tauri/binaries/legibility-chat-mcp-${TARGET}"

echo "Building legibility-chat-mcp sidecar..."
cargo build -p legibility-chat-mcp

cp "$BUILD_DIR/legibility-chat-mcp" "$SIDECAR"
echo "Sidecar copied to $SIDECAR"

echo "Freeing port 5173..."
fuser -k 5173/tcp 2>/dev/null || true

echo "Launching app..."
tauri dev
