#!/usr/bin/env bash
set -euo pipefail

profile="${1:?usage: run-profile.sh <default|disable-dmabuf|force-dmabuf|force-dmabuf-shm>}"
out="${2:-/tmp/opencode/poc-gui/$(date +%Y%m%d-%H%M%S)-$profile}"
mkdir -p "$out"

case "$profile" in
  default)          envs=() ;;
  disable-dmabuf)   envs=(WEBKIT_DISABLE_DMABUF_RENDERER=1) ;;
  force-dmabuf)     envs=(WEBKIT_FORCE_DMABUF_RENDERER=1) ;;
  force-dmabuf-shm) envs=(WEBKIT_FORCE_DMABUF_RENDERER=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1) ;;
  *) echo "profil inconnu: $profile" >&2; exit 2 ;;
esac

gui_bin="${GUI_BIN:-./target/release/workflow-gui-tauri}"
server_url="${SERVER_URL:-http://127.0.0.1:5005}"
audit_js="${AUDIT_JS:-./scripts/poc/audit.js}"

env "${envs[@]}" "$gui_bin" \
  --server-url "$server_url" \
  --telemetry-dir "$out" \
  --ready-file "$out/ready" \
  --poc-driver "$audit_js" \
  >"$out/gui.log" 2>&1 &
gui_pid=$!

python3 scripts/poc/measure-gui.py \
  --pid "$gui_pid" --profile "$profile" --phase baseline \
  --duration "${DURATION:-60}" --interval 1 \
  --out "$out" --ready-file "$out/ready" || true

kill -TERM "$gui_pid" 2>/dev/null || true
for _ in $(seq 1 50); do
  kill -0 "$gui_pid" 2>/dev/null || break
  sleep 0.1
done
kill -KILL "$gui_pid" 2>/dev/null || true
wait "$gui_pid" 2>/dev/null || true
