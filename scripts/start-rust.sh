#!/usr/bin/env bash
# Lanceur interactif du serveur Rust, utilisé par le raccourci du Bureau.
#
# Il reste strictement séparé du legacy Python : port 5004, runtime Rust,
# verrou /tmp/workflow-rs-hw.lock et secrets locaux dans config/launcher.env.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="${WORKFLOW_RUST_CONFIG:-$ROOT/config/config.toml}"
ENV_FILE="${WORKFLOW_RUST_ENV_FILE:-$ROOT/config/launcher.env}"
LOG_FILE="${WORKFLOW_RUST_LOG:-$ROOT/runtime/logs/workflow-server.log}"
URL="${WORKFLOW_RUST_URL:-http://127.0.0.1:5004}"
BROWSER_CMD="${BROWSER_CMD:-firefox}"

pause_on_error() {
    if [[ -t 0 ]]; then
        read -r -p "Appuyez sur Entrée pour fermer..." _ || true
    fi
}

if [[ ! -f "$ENV_FILE" ]]; then
    echo "Création de $ENV_FILE (secrets locaux, non versionnés)..."
    umask 077
    secret="$(openssl rand -hex 32)"
    token="$(openssl rand -hex 32)"
    cat >"$ENV_FILE" <<EOF
# Secrets locaux du serveur Rust. Ne pas versionner.
WORKFLOW_SERVER_SECRET_KEY=$secret
WORKFLOW_SERVER_WORKER_TOKEN=$token
EOF
    chmod 600 "$ENV_FILE"
fi

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

if [[ -z "${WORKFLOW_SERVER_WORKER_TOKEN:-}" ]]; then
    echo "WORKFLOW_SERVER_WORKER_TOKEN absent de $ENV_FILE" >&2
    pause_on_error
    exit 1
fi

if [[ ! -x "$ROOT/target/release/workflow-server" ]]; then
    echo "Binaire absent : $ROOT/target/release/workflow-server" >&2
    echo "Construire d'abord :" >&2
    echo "  cd $ROOT && cargo build --release -p workflow-server -p workflow-cli" >&2
    pause_on_error
    exit 1
fi

echo "Vérification de la configuration Rust..."
if ! "$ROOT/scripts/switch-rust.sh" check; then
    echo "La vérification a échoué ; le serveur n'est pas démarré." >&2
    pause_on_error
    exit 1
fi

webhook_enabled="$(sed -n '/^\[webhook\]/,/^\[/ s/^enabled = \(.*\)/\1/p' "$CONFIG" 2>/dev/null | head -n 1 | tr -d '[:space:]')"
rust_running=0
if "$ROOT/scripts/switch-rust.sh" running; then
    rust_running=1
fi
if [[ "$rust_running" == "0" && "$webhook_enabled" == "true" && "${WORKFLOW_SYNC_ON_START:-1}" != "0" ]]; then
    echo
    echo "Synchronisation de l'état Python legacy vers Rust..."
    # shellcheck disable=SC2086
    if ! "$ROOT/scripts/sync-backends.sh" --apply --quiet ${WORKFLOW_SYNC_ARGS:-}; then
        echo "La synchronisation a échoué ; le serveur n'est pas démarré." >&2
        echo "Corriger l'erreur, ou relancer avec WORKFLOW_SYNC_ON_START=0." >&2
        pause_on_error
        exit 1
    fi
fi

echo
echo "Démarrage du serveur Rust sur $URL ..."
# Ne pas rejouer les anciennes lignes du journal dans le terminal : elles
# peuvent contenir des erreurs webhook déjà résolues et donner l'impression
# qu'elles viennent du lancement courant.
LOG_START_LINE=1
if [[ -f "$LOG_FILE" ]]; then
    LOG_START_LINE=$(( $(wc -l < "$LOG_FILE" 2>/dev/null || echo 0) + 1 ))
fi
if ! "$ROOT/scripts/switch-rust.sh" start; then
    echo "Le serveur n'a pas démarré ; voir $LOG_FILE" >&2
    pause_on_error
    exit 1
fi

if [[ "${WORKFLOW_RUST_NO_BROWSER:-0}" != "1" ]]; then
    if command -v "$BROWSER_CMD" >/dev/null 2>&1; then
        nohup "$BROWSER_CMD" "$URL/" >/dev/null 2>&1 &
    else
        xdg-open "$URL/" >/dev/null 2>&1 || true
    fi
fi

echo
echo "Interface Rust : $URL/"
echo "Logs          : $LOG_FILE"
echo "Ctrl+C ou fermeture du terminal arrête le serveur."
echo

if [[ "${WORKFLOW_RUST_NO_TAIL:-0}" == "1" ]]; then
    exit 0
fi

SERVER_PID="$(cat "$ROOT/runtime/workflow-server.pid" 2>/dev/null || true)"
if [[ -z "$SERVER_PID" ]]; then
    echo "PID du serveur introuvable ; voir $LOG_FILE" >&2
    pause_on_error
    exit 1
fi

# `--pid` fait sortir tail dès que le serveur s'arrête, même si tail ignore
# SIGINT en tant que tâche de fond. On repart de la ligne comptée avant le
# démarrage pour n'afficher que la session courante.
tail -n +"$LOG_START_LINE" -f --pid="$SERVER_PID" "$LOG_FILE" &
TAIL_PID=$!

STOPPED=0
cleanup() {
    STOPPED=1
    kill "$TAIL_PID" 2>/dev/null || true
    echo
    echo "Arrêt du serveur Rust..."
    "$ROOT/scripts/switch-rust.sh" stop || true
}
trap cleanup INT TERM HUP

wait "$TAIL_PID" || true
if [[ "$STOPPED" == "0" ]]; then
    "$ROOT/scripts/switch-rust.sh" stop || true
fi
