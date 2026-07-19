#!/usr/bin/env bash
set -euo pipefail
PORT="${TAURI_DEV_PORT:-1420}"
if command -v lsof >/dev/null 2>&1; then
  pid=$(lsof -tiTCP:"$PORT" -sTCP:LISTEN 2>/dev/null | head -n 1 || true)
  if [[ -n "$pid" ]]; then
    kill "$pid" 2>/dev/null || true
  fi
fi
