#!/usr/bin/env bash
# Démarrage contrôlé du serveur Rust, sans toucher au serveur Python legacy.
#
# Le basculement du trafic (reverse proxy, DNS ou port public) reste une
# décision externe : ce script lance et vérifie Rust sur le port 5004 pendant
# que Python reste disponible sur 5003 pour le rollback.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG="${WORKFLOW_RUST_CONFIG:-$ROOT/config/config.toml}"
BIN="${WORKFLOW_RUST_BIN:-$ROOT/target/release/workflow-server}"
PID_FILE="${WORKFLOW_RUST_PID_FILE:-$ROOT/runtime/workflow-server.pid}"
LOG_FILE="${WORKFLOW_RUST_LOG:-$ROOT/runtime/logs/workflow-server.log}"
RUST_URL="${WORKFLOW_RUST_URL:-http://127.0.0.1:5004}"
LEGACY_URL="${WORKFLOW_LEGACY_URL:-http://127.0.0.1:5003}"
TOKEN="${WORKFLOW_SERVER_WORKER_TOKEN:-}"

usage() {
    cat <<EOF
Usage: $0 {check|start|stop|status}

Environment:
  WORKFLOW_SERVER_WORKER_TOKEN   obligatoire pour check/start/status
  WORKFLOW_SERVER_SECRET_KEY     secret du serveur Rust
  WORKFLOW_RUST_CONFIG           défaut: $CONFIG
  WORKFLOW_RUST_URL              défaut: $RUST_URL
  WORKFLOW_LEGACY_URL            défaut: $LEGACY_URL

Le serveur Rust utilise son propre runtime, sa propre base, le port 5004 et
/tmp/workflow-rs-hw.lock. Aucun chemin Python n'est modifié.
EOF
}

require_token() {
    if [[ -z "$TOKEN" ]]; then
        echo "WORKFLOW_SERVER_WORKER_TOKEN est requis" >&2
        exit 2
    fi
}

ping_rust() {
    curl -fs --max-time 2 -H "X-Worker-Token: $TOKEN" "$RUST_URL/api/ping" >/dev/null 2>&1
}

ping_legacy() {
    curl -fs --max-time 2 "$LEGACY_URL/" >/dev/null 2>&1
}

is_running() {
    [[ -f "$PID_FILE" ]] || return 1
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

case "${1:-}" in
    check)
        require_token
        if [[ ! -x "$BIN" ]]; then
            echo "binaire absent: $BIN (cargo build --release -p workflow-server)" >&2
            exit 1
        fi
        env WORKFLOW_SERVER_WORKER_TOKEN="$TOKEN" \
            WORKFLOW_SERVER_SECRET_KEY="${WORKFLOW_SERVER_SECRET_KEY:-}" \
            "$BIN" --config "$CONFIG" --check

        cli_path="$(sed -n 's/^workflow_cli = "\(.*\)"/\1/p' "$CONFIG" | head -n 1)"
        if [[ -n "$cli_path" && -x "$cli_path" ]]; then
            echo "workflow-cli: $cli_path"
        else
            echo "workflow-cli absent ou non exécutable: ${cli_path:-non configuré}" >&2
        fi

        source_dir="$(sed -n '/^\[steps\.step1\]/,/^\[/ s/^source_dir = "\(.*\)"/\1/p' "$CONFIG" | head -n 1)"
        if [[ -n "$source_dir" && -d "$source_dir" ]]; then
            echo "source STEP1: $source_dir"
        else
            echo "source STEP1 absente: ${source_dir:-non configurée} (STEP1 échouera)" >&2
        fi

        for tool in ffmpeg ffprobe effectcraft-cli; do
            if command -v "$tool" >/dev/null 2>&1; then
                echo "outil $tool: OK"
            else
                echo "outil $tool: absent (étapes concernées en échec)" >&2
            fi
        done
        if command -v unrar >/dev/null 2>&1; then
            echo "outil unrar: OK (archives RAR)"
        else
            echo "outil unrar: absent (archives RAR en échec)" >&2
        fi

        for step in step3 step4; do
            for key in tflite edgetpu; do
                lib="$(sed -n "/^\\[steps\\.$step\\]/,/^\\[/ s/^$key = \"\\(.*\\)\"/\\1/p" "$CONFIG" | head -n 1)"
                if [[ -n "$lib" && -r "$lib" ]]; then
                    echo "bibliothèque $step.$key: OK"
                else
                    echo "bibliothèque $step.$key absente: ${lib:-non configurée}" >&2
                fi
            done
        done

        if is_running && ping_rust; then
            echo "Rust répond sur $RUST_URL"
        else
            echo "Rust n'est pas démarré (normal avant start)"
        fi
        if ping_legacy; then
            echo "Python legacy répond sur $LEGACY_URL (rollback disponible)"
        else
            echo "Python legacy ne répond pas sur $LEGACY_URL (rollback indisponible)" >&2
        fi
        ;;
    start)
        require_token
        if is_running; then
            echo "workflow-server déjà démarré (PID $(cat "$PID_FILE"))"
            exit 0
        fi
        env WORKFLOW_SERVER_WORKER_TOKEN="$TOKEN" \
            WORKFLOW_SERVER_SECRET_KEY="${WORKFLOW_SERVER_SECRET_KEY:-}" \
            "$BIN" --config "$CONFIG" --check >/dev/null
        mkdir -p "$(dirname "$PID_FILE")" "$(dirname "$LOG_FILE")"
        env WORKFLOW_SERVER_WORKER_TOKEN="$TOKEN" \
            WORKFLOW_SERVER_SECRET_KEY="${WORKFLOW_SERVER_SECRET_KEY:-}" \
            nohup "$BIN" --config "$CONFIG" >>"$LOG_FILE" 2>&1 &
        echo $! >"$PID_FILE"
        for _ in $(seq 1 30); do
            if ping_rust; then
                echo "workflow-server démarré sur $RUST_URL (PID $(cat "$PID_FILE"))"
                if ping_legacy; then
                    echo "Python legacy reste actif sur $LEGACY_URL"
                else
                    echo "Python legacy ne répond pas sur $LEGACY_URL (rollback à vérifier)"
                fi
                exit 0
            fi
            sleep 0.5
        done
        echo "workflow-server n'a pas répondu; voir $LOG_FILE" >&2
        exit 1
        ;;
    stop)
        if ! is_running; then
            echo "workflow-server n'est pas démarré"
            rm -f "$PID_FILE"
            exit 0
        fi
        pid="$(cat "$PID_FILE")"
        kill "$pid"
        for _ in $(seq 1 20); do
            if ! kill -0 "$pid" 2>/dev/null; then
                rm -f "$PID_FILE"
                echo "workflow-server arrêté"
                exit 0
            fi
            sleep 0.5
        done
        echo "arrêt forcé de $pid" >&2
        kill -9 "$pid" 2>/dev/null || true
        rm -f "$PID_FILE"
        ;;
    status)
        require_token
        if is_running; then
            echo "PID: $(cat "$PID_FILE")"
        else
            echo "PID: arrêté"
        fi
        if ping_rust; then
            echo "Rust: $RUST_URL OK"
        else
            echo "Rust: $RUST_URL indisponible"
        fi
        if ping_legacy; then
            echo "Legacy: $LEGACY_URL OK (rollback)"
        else
            echo "Legacy: $LEGACY_URL indisponible"
        fi
        ;;
    *)
        usage
        exit 2
        ;;
esac
