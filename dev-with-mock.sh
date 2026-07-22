#!/usr/bin/env bash
set -e

MOCK_PORT="${MOCK_PORT:-8127}"

echo "Starting mock FLEX API server on port $MOCK_PORT..."
node mock-server.js "$MOCK_PORT" &
MOCK_PID=$!

cleanup() {
  echo "Stopping mock server (pid $MOCK_PID)..."
  kill "$MOCK_PID" 2>/dev/null || true
}
trap cleanup EXIT

./dev.sh
