#!/usr/bin/env bash
# Installe le serveur workflow-rs comme service systemd --user.
#
# Le service est indépendant du GUI : le client desktop ne démarre, n'arrête
# et ne surveille jamais le processus serveur. Le PoC peut aussi utiliser
# scripts/switch-rust.sh ; les deux modes ne doivent pas tourner ensemble.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/packaging/systemd/workflow-server.service.in"
CONFIG="$ROOT/config/config.toml"
ENV_SOURCE="$ROOT/config/launcher.env"
UNIT_NAME="workflow-server"
ENV_DEST=""
URL="http://127.0.0.1:5004"
START=0
UNINSTALL=0

usage() {
    cat <<EOF
Usage: $0 [options]

Installe (ou retire) workflow-server comme unité systemd --user.

Options:
  --config PATH       configuration du serveur (défaut: $CONFIG)
  --env-file PATH     fichier de secrets (défaut: $ENV_SOURCE)
  --env-dest PATH     destination des secrets
                      (défaut: ~/.config/workflow-rs/launcher.env)
  --unit-name NAME    nom de l'unité sans .service (défaut: $UNIT_NAME)
  --url URL           URL de contrôle /api/ping (défaut: $URL)
  --start             démarre l'unité après installation (préflight inclus)
  --uninstall         arrête, désactive et supprime l'unité
  -h, --help          affiche cette aide

Le script ne démarre jamais le serveur sans --start. Il refuse de démarrer
si le webhook est activé et que Python répond sur :5003, ou si le port du
serveur est déjà occupé par un autre processus.
EOF
}

log() { printf '%s\n' "$*"; }
die() { printf 'ERREUR: %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --config) CONFIG="${2:?}"; shift 2 ;;
        --env-file) ENV_SOURCE="${2:?}"; shift 2 ;;
        --env-dest) ENV_DEST="${2:?}"; shift 2 ;;
        --unit-name) UNIT_NAME="${2:?}"; shift 2 ;;
        --url) URL="${2:?}"; shift 2 ;;
        --start) START=1; shift ;;
        --uninstall) UNINSTALL=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) die "option inconnue: $1" ;;
    esac
done

command -v systemctl >/dev/null 2>&1 || die "systemctl absent"
systemctl --user show-environment >/dev/null 2>&1 || die "systemd --user indisponible"

CONFIG="$(readlink -f "$CONFIG")"
ENV_SOURCE="$(readlink -f "$ENV_SOURCE")"
[[ -f "$CONFIG" ]] || die "configuration absente: $CONFIG"
[[ -f "$ENV_SOURCE" ]] || die "fichier de secrets absent: $ENV_SOURCE"

UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
if [[ -z "$ENV_DEST" ]]; then
    ENV_DEST="${XDG_CONFIG_HOME:-$HOME/.config}/workflow-rs/launcher.env"
fi
UNIT_PATH="$UNIT_DIR/$UNIT_NAME.service"

if [[ "$UNINSTALL" == "1" ]]; then
    systemctl --user disable --now "$UNIT_NAME.service" >/dev/null 2>&1 || true
    rm -f "$UNIT_PATH"
    systemctl --user daemon-reload
    systemctl --user reset-failed "$UNIT_NAME.service" >/dev/null 2>&1 || true
    log "unité retirée: $UNIT_PATH"
    log "secrets conservés: $ENV_DEST"
    exit 0
fi

BIN="$ROOT/target/release/workflow-server"
[[ -x "$BIN" ]] || die "binaire absent: $BIN (cargo build --release -p workflow-server)"

MODE="$(stat -c '%a' "$ENV_SOURCE")"
if [[ "$MODE" != "600" ]]; then
    die "secrets $ENV_SOURCE en mode $MODE, attendu 600"
fi

# Les chemins sont injectés dans une unité systemd : refuser les caractères
# qui casseraient sed ou la syntaxe de l'unité.
case "$ROOT$CONFIG$ENV_DEST" in
    *'|'*|*'&'*|*$'\n'*) die "chemin non supporté (|, & ou saut de ligne)" ;;
esac

# Préflight avant toute installation : ne jamais lancer deux moniteurs
# webhook ni écraser un port. Une unité déjà active est laissée telle quelle.
if [[ "$START" == "1" ]] && ! systemctl --user is-active --quiet "$UNIT_NAME.service"; then
    PORT="$(sed -n '/^\[server\]/,/^\[/ s/^port = \(.*\)/\1/p' "$CONFIG" | head -n 1 | tr -d '[:space:]')"
    PORT="${PORT:-5004}"
    HOST="$(sed -n '/^\[server\]/,/^\[/ s/^host = "\(.*\)"/\1/p' "$CONFIG" | head -n 1)"
    HOST="${HOST:-127.0.0.1}"
    if [[ "$HOST" == "0.0.0.0" || "$HOST" == "::" ]]; then
        HOST="127.0.0.1"
    fi
    WEBHOOK="$(sed -n '/^\[webhook\]/,/^\[/ s/^enabled = \(.*\)/\1/p' "$CONFIG" | head -n 1 | tr -d '[:space:]')"
    if [[ "$WEBHOOK" == "true" ]] && curl -s -o /dev/null --max-time 2 "http://127.0.0.1:5003/"; then
        die "Python répond sur :5003 alors que le webhook est activé; arrêter Python et synchroniser avant de démarrer"
    fi
    if curl -s -o /dev/null --max-time 1 "http://$HOST:$PORT/"; then
        die "le port $PORT répond déjà; arrêter l'autre serveur avant de démarrer $UNIT_NAME.service"
    fi
fi

mkdir -p "$UNIT_DIR" "$(dirname "$ENV_DEST")" "$ROOT/runtime"
if [[ "$ENV_SOURCE" != "$ENV_DEST" ]]; then
    install -m 600 "$ENV_SOURCE" "$ENV_DEST"
fi

sed -e "s|@ROOT@|$ROOT|g" \
    -e "s|@CONFIG@|$CONFIG|g" \
    -e "s|@ENV_FILE@|$ENV_DEST|g" \
    "$TEMPLATE" >"$UNIT_PATH"
chmod 644 "$UNIT_PATH"

systemctl --user daemon-reload
log "unité installée: $UNIT_PATH"
log "secrets: $ENV_DEST (0600)"

if [[ "$START" != "1" ]]; then
    cat <<EOF
Démarrer:        systemctl --user start $UNIT_NAME.service
Activer:         systemctl --user enable $UNIT_NAME.service
État:            systemctl --user status $UNIT_NAME.service
Journaux:        journalctl --user -u $UNIT_NAME.service -f
Arrêter:         systemctl --user stop $UNIT_NAME.service
Désinstaller:    $0 --unit-name $UNIT_NAME --uninstall
EOF
    exit 0
fi

if systemctl --user is-active --quiet "$UNIT_NAME.service"; then
    log "$UNIT_NAME.service déjà actif"
else
    systemctl --user start "$UNIT_NAME.service"
fi

set -a
# shellcheck disable=SC1090
source "$ENV_DEST"
set +a
[[ -n "${WORKFLOW_SERVER_WORKER_TOKEN:-}" ]] || die "WORKFLOW_SERVER_WORKER_TOKEN absent de $ENV_DEST"

for _ in $(seq 1 30); do
    if curl -fs --max-time 2 -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" "${URL%/}/api/ping" >/dev/null 2>&1; then
        log "serveur prêt sur ${URL%/} (unité $UNIT_NAME.service)"
        exit 0
    fi
    sleep 0.5
done

systemctl --user status --no-pager "$UNIT_NAME.service" >&2 || true
die "le serveur n'a pas répondu sur ${URL%/}/api/ping"
