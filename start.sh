#!/bin/sh
set -eu

# Start arti in the background
./arti proxy -c /etc/arti/onionservice.toml &
ARTI_PID=$!

cleanup() {
  # Propagate termination to arti
  if kill -0 "$ARTI_PID" 2>/dev/null; then
    kill "$ARTI_PID" 2>/dev/null || true
    wait "$ARTI_PID" 2>/dev/null || true
  fi
}

trap cleanup INT TERM EXIT

# Start the Axum webserver in the foreground
exec ./arti-railway
