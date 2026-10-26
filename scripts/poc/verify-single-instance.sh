#!/usr/bin/env bash
# Vérifie le single-instance D-Bus du client desktop (MVP J1).
#
# Le script lance une instance primaire, minimise sa fenêtre, puis vérifie
# qu'un second lancement :
#   - se termine immédiatement (code 0) sans créer de second processus ;
#   - restaure et focalise la fenêtre existante ;
#   - rejette une URL serveur divergente avec InvalidArgs ;
#   - fonctionne même si le serveur HTTP est arrêté (option --stop-server).
#
# Prérequis : session graphique X11, `gdbus`, `wmctrl`, `xdotool`, `xprop`,
# `dbus-monitor`, et un serveur workflow-server déjà démarré.
#
# Exemple :
#   scripts/poc/verify-single-instance.sh \
#       --gui target/release/workflow-gui-tauri \
#       --server-url http://127.0.0.1:5005 \
#       --server-pid-file /tmp/opencode/mvp-j1/server.pid \
#       --stop-server
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GUI="$ROOT/target/release/workflow-gui-tauri"
SERVER_URL="http://127.0.0.1:5005"
SERVER_PID_FILE=""
OUT="/tmp/opencode/verify-single-instance"
STOP_SERVER=0

usage() {
    cat <<EOF
Usage: $0 [options]

Options:
  --gui PATH             binaire workflow-gui-tauri (défaut: $GUI)
  --server-url URL       URL du serveur (défaut: $SERVER_URL)
  --server-pid-file PATH PID file du serveur pour le test hors ligne
  --out DIR              dossier des preuves (défaut: $OUT)
  --stop-server          arrête le serveur après les tests en ligne
  -h, --help             affiche cette aide
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --gui) GUI="${2:?}"; shift 2 ;;
        --server-url) SERVER_URL="${2:?}"; shift 2 ;;
        --server-pid-file) SERVER_PID_FILE="${2:?}"; shift 2 ;;
        --out) OUT="${2:?}"; shift 2 ;;
        --stop-server) STOP_SERVER=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'option inconnue: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

for tool in gdbus wmctrl xdotool xprop dbus-monitor; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'outil requis absent: %s\n' "$tool" >&2
        exit 2
    }
done
[[ -x "$GUI" ]] || { printf 'GUI introuvable: %s\n' "$GUI" >&2; exit 2; }
if [[ "$STOP_SERVER" == "1" ]]; then
    [[ -n "$SERVER_PID_FILE" && -f "$SERVER_PID_FILE" ]] || {
        printf '%s\n' '--stop-server exige --server-pid-file existant' >&2
        exit 2
    }
fi

rm -rf "$OUT"
mkdir -p "$OUT/telemetry"
PRIMARY_PID=""
MONITOR_PID=""
FAIL=0

cleanup() {
    if [[ -n "$MONITOR_PID" ]]; then kill "$MONITOR_PID" 2>/dev/null || true; fi
    if [[ -n "$PRIMARY_PID" ]]; then kill -TERM "$PRIMARY_PID" 2>/dev/null || true; fi
}
trap cleanup EXIT

log() { printf '%s\n' "$*" | tee -a "$OUT/report.txt"; }
fail() { log "FAIL: $*"; FAIL=1; }

"$GUI" --server-url "$SERVER_URL" \
    --telemetry-dir "$OUT/telemetry" --no-tray --no-close-to-tray \
    >"$OUT/primary.out" 2>"$OUT/primary.err" &
PRIMARY_PID=$!
log "primary_pid=$PRIMARY_PID"

for _ in $(seq 1 80); do
    [[ -f "$OUT/telemetry/page-loaded.txt" ]] && break
    sleep 0.25
done
log "page=$(cat "$OUT/telemetry/page-loaded.txt" 2>/dev/null || echo absent)"
[[ -f "$OUT/telemetry/page-loaded.txt" ]] || fail "page non chargée"

WID=$(wmctrl -l | awk '/Workflow Manager/{print $1; exit}')
log "window_id=$WID"
[[ -n "$WID" ]] || { log "ERREUR: fenêtre absente"; exit 1; }

log "--- propriétaire du nom"
gdbus call --session --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetNameOwner org.workflow.desktop \
    | tee -a "$OUT/report.txt"

log "--- introspection org.freedesktop.Application"
gdbus introspect --session --dest org.workflow.desktop \
    --object-path /org/workflow/desktop \
    | tee "$OUT/introspect.txt"
log "introspect_bytes=$(wc -c < "$OUT/introspect.txt")"

log "--- état initial"
xprop -id "$WID" WM_CLASS WM_NAME _NET_WM_STATE | tee -a "$OUT/report.txt"
xwininfo -id "$WID" | grep -E 'Map State|Absolute|Width|Height' | tee -a "$OUT/report.txt"

xdotool windowactivate --sync "$WID" 2>"$OUT/windowactivate.err" || true
sleep 0.5
log "focus_after_activate=$(xdotool getwindowfocus 2>&1 || true)"
log "active_window_after_activate=$(xprop -root _NET_ACTIVE_WINDOW 2>&1 | head -1)"

log "--- minimisation"
xdotool windowminimize "$WID"
MINIMIZED=no
for _ in $(seq 1 30); do
    if xprop -id "$WID" _NET_WM_STATE 2>/dev/null | grep -q '_NET_WM_STATE_HIDDEN'; then
        MINIMIZED=yes
        break
    fi
    sleep 0.2
done
log "minimized_hidden_state=$MINIMIZED"
[[ "$MINIMIZED" == "yes" ]] || fail "la fenêtre n'a pas été minimisée"

log "--- second lancement avec trace D-Bus"
dbus-monitor --session "interface='org.freedesktop.Application'" \
    >"$OUT/dbus-monitor.log" 2>&1 &
MONITOR_PID=$!
sleep 0.5
START_MS=$(date +%s%3N)
set +e
"$GUI" --server-url "$SERVER_URL" --telemetry-dir "$OUT/telemetry" \
    --no-tray --no-close-to-tray >"$OUT/second.out" 2>"$OUT/second.err"
SECOND_RC=$?
set -e
ELAPSED_MS=$(( $(date +%s%3N) - START_MS ))
sleep 1.2
kill "$MONITOR_PID" 2>/dev/null || true
MONITOR_PID=""
log "second_rc=$SECOND_RC elapsed_ms=$ELAPSED_MS"
[[ "$SECOND_RC" == "0" ]] || fail "le second lancement a échoué"
ACTIVATIONS=$(grep -c "activation d'une instance existante" "$OUT/primary.err" || true)
log "second_activations=$ACTIVATIONS"
[[ "$ACTIVATIONS" -ge 1 ]] || fail "activation non reçue"
GUI_PROCESSES=$(pgrep -fc "^$GUI" || true)
log "gui_processes=$GUI_PROCESSES"
[[ "$GUI_PROCESSES" == "1" ]] || fail "plus d'une instance graphique"
HIDDEN_AFTER=$(xprop -id "$WID" _NET_WM_STATE | grep -o '_NET_WM_STATE_HIDDEN' || true)
FOCUSED_AFTER=$(xprop -id "$WID" _NET_WM_STATE | grep -o '_NET_WM_STATE_FOCUSED' || true)
MAP_AFTER=$(xwininfo -id "$WID" | awk -F: '/Map State/{gsub(/^ +/, "", $2); print $2}')
log "hidden_after_second=${HIDDEN_AFTER:-none}"
log "focused_state_after_second=${FOCUSED_AFTER:-none}"
log "map_state_after_second=$MAP_AFTER"
log "focus_after_second=$(xdotool getwindowfocus 2>&1 || true)"
[[ -z "$HIDDEN_AFTER" ]] || fail "fenêtre restée minimisée"
[[ -n "$FOCUSED_AFTER" ]] || fail "fenêtre sans focus"
[[ "$MAP_AFTER" == "IsViewable" ]] || fail "fenêtre non visible"
log "dbus_activate_calls=$(grep -c 'member=Activate' "$OUT/dbus-monitor.log" || true)"

log "--- URL divergente"
set +e
"$GUI" --server-url http://127.0.0.1:5999 --telemetry-dir "$OUT/telemetry" \
    --no-tray --no-close-to-tray >"$OUT/divergent.out" 2>"$OUT/divergent.err"
DIVERGENT_RC=$?
set -e
log "divergent_rc=$DIVERGENT_RC"
[[ "$DIVERGENT_RC" == "1" ]] || fail "URL divergente acceptée"
if grep -q 'org.freedesktop.DBus.Error.InvalidArgs' "$OUT/divergent.err"; then
    log "divergent_invalid_args=yes"
else
    log "divergent_invalid_args=no"
    fail "InvalidArgs absent"
fi

if [[ "$STOP_SERVER" == "1" ]]; then
    log "--- second lancement avec serveur arrêté"
    SERVER_PID=$(cat "$SERVER_PID_FILE")
    kill -TERM "$SERVER_PID" 2>/dev/null || true
    for _ in $(seq 1 40); do
        kill -0 "$SERVER_PID" 2>/dev/null || break
        sleep 0.25
    done
    ACTIVATIONS_BEFORE=$(grep -c "activation d'une instance existante" "$OUT/primary.err" || true)
    set +e
    "$GUI" --server-url "$SERVER_URL" --telemetry-dir "$OUT/telemetry" \
        --no-tray --no-close-to-tray >"$OUT/second-offline.out" 2>"$OUT/second-offline.err"
    OFFLINE_RC=$?
    set -e
    sleep 0.5
    ACTIVATIONS_AFTER=$(grep -c "activation d'une instance existante" "$OUT/primary.err" || true)
    log "offline_second_rc=$OFFLINE_RC activations_before=$ACTIVATIONS_BEFORE activations_after=$ACTIVATIONS_AFTER"
    [[ "$OFFLINE_RC" == "0" ]] || fail "second lancement hors ligne en échec"
    [[ "$ACTIVATIONS_AFTER" -gt "$ACTIVATIONS_BEFORE" ]] || fail "activation hors ligne non reçue"
fi

log "--- arrêt de l'instance primaire"
kill -TERM "$PRIMARY_PID"
for _ in $(seq 1 40); do
    kill -0 "$PRIMARY_PID" 2>/dev/null || break
    sleep 0.25
done
PRIMARY_PID=""
set +e
gdbus call --session --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetNameOwner org.workflow.desktop \
    >"$OUT/name-owner-after.txt" 2>"$OUT/name-owner-after.err"
NAME_RC=$?
set -e
log "name_owner_after_rc=$NAME_RC"
[[ "$NAME_RC" != "0" ]] || fail "nom D-Bus encore possédé"
tee -a "$OUT/report.txt" < "$OUT/name-owner-after.err" || true

if [[ "$FAIL" == "0" ]]; then
    log "RESULT=OK"
else
    log "RESULT=FAIL"
    exit 1
fi
