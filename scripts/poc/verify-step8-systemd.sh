#!/usr/bin/env bash
# STEP8 sous unité systemd --user isolée (MVP J2).
#
# Crée une fixture projet sous /tmp/opencode, installe une unité de test avec
# le même durcissement que la production, exécute STEP8 via le serveur, puis
# vérifie le .ecproj, les permissions et l'absence d'écriture dans /mnt/cache.
set -euo pipefail

ROOT_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT="/tmp/opencode/mvp-j2/step8-$(date +%Y%m%d-%H%M%S)"
UNIT="workflow-server-step8-test"
PORT=5007

usage() {
    cat <<EOF
Usage: $0 [options]

Options:
  --out DIR          dossier de travail (défaut: $ROOT)
  --unit NAME        nom de l'unité de test (défaut: $UNIT)
  --port PORT        port HTTP isolé (défaut: $PORT)
  -h, --help         affiche cette aide

Le script refuse de démarrer si un serveur de production ou un GUI est actif.
Il conserve les artefacts et retire l'unité de test à la fin.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out) ROOT="${2:?}"; shift 2 ;;
        --unit) UNIT="${2:?}"; shift 2 ;;
        --port) PORT="${2:?}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'option inconnue: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

STATE="$ROOT/state"
CACHE="$ROOT/cache"
PROJECT="$STATE/projets_extraits/1531 Camille"
CONFIG="$ROOT/config.toml"
ENV_FILE="$ROOT/launcher.env"
WRAPPER="$ROOT/child-wrapper.sh"
UNIT_PATH="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/$UNIT.service"

fail() {
    printf 'ERREUR: %s\n' "$*" >&2
    exit 1
}

log() { printf '%s\n' "$*"; }

cleanup() {
    systemctl --user stop "$UNIT.service" >/dev/null 2>&1 || true
    "$ROOT_REPO/scripts/install-systemd-user.sh" --unit-name "$UNIT" --uninstall \
        >/dev/null 2>&1 || true
    systemctl --user reset-failed "$UNIT.service" >/dev/null 2>&1 || true
}
trap cleanup EXIT

command -v systemctl >/dev/null 2>&1 || fail "systemctl absent"
systemctl --user show-environment >/dev/null 2>&1 || fail "systemd --user indisponible"
for tool in curl jq python3 systemd-analyze effectcraft-cli; do
    command -v "$tool" >/dev/null 2>&1 || fail "outil absent: $tool"
done
for binary in workflow-server workflow-cli; do
    [[ -x "$ROOT_REPO/target/release/$binary" ]] \
        || fail "binaire absent: $ROOT_REPO/target/release/$binary"
done
[[ -x "$ROOT_REPO/scripts/install-systemd-user.sh" ]] \
    || fail "script d'installation systemd absent"

case "$ROOT" in
    /mnt/cache*|"$ROOT_REPO/runtime"*|"/")
        fail "sortie interdite: $ROOT"
        ;;
esac

if systemctl --user is-active --quiet workflow-server.service 2>/dev/null; then
    fail "workflow-server.service est actif; arrêter la production avant le test"
fi
if pgrep -x workflow-gui-tauri >/dev/null 2>&1; then
    fail "une instance workflow-gui-tauri est active"
fi
if ss -ltn 2>/dev/null | grep -q ":$PORT "; then
    fail "port $PORT déjà occupé"
fi

rm -rf "$ROOT"
mkdir -p "$PROJECT/docs" "$STATE/logs/step7" "$STATE/archives" \
    "$STATE/downloads" "$STATE/logs/step8" "$CACHE"
cp "$ROOT_REPO/parity/cases/short_2997.mp4" "$PROJECT/docs/"
cp "$ROOT_REPO/parity/fixtures/short_2997_step2/expected/short_2997.csv" \
    "$PROJECT/docs/short_2997.csv"

MANIFEST="$STATE/logs/step7/finalized_projects_$(date +%Y%m%d_%H%M%S).json"
cat > "$MANIFEST" <<EOF
{"schema":1,
 "output_dir":"$STATE/projets_extraits",
 "projects":[{"name":"1531 Camille","path":"$PROJECT"}]}
EOF

# Wrapper de traçabilité : il consigne la présence des variables, pas leur
# valeur, puis exécute le vrai CLI release.
cat > "$WRAPPER" <<EOF
#!/usr/bin/env bash
set -euo pipefail
{
    echo "=== \$(date -Is) ==="
    echo "pid=\$\$"
    echo "cwd=\$PWD"
    echo "argv=\$*"
    echo "CUDA_CACHE_PATH=\${CUDA_CACHE_PATH-<unset>}"
    echo "CUDA_CACHE_MAXSIZE=\${CUDA_CACHE_MAXSIZE-<unset>}"
    echo "CACHE_DIRECTORY=\${CACHE_DIRECTORY-<unset>}"
    echo "WORKFLOW_SERVER_WORKER_TOKEN=\${WORKFLOW_SERVER_WORKER_TOKEN-<unset>}"
    echo "WORKFLOW_SERVER_SECRET_KEY=\${WORKFLOW_SERVER_SECRET_KEY-<unset>}"
    echo "FLASK_SECRET_KEY=\${FLASK_SECRET_KEY-<unset>}"
    echo "INTERNAL_WORKER_COMMS_TOKEN=\${INTERNAL_WORKER_COMMS_TOKEN-<unset>}"
} >> "$ROOT/child-env.txt"
exec "$ROOT_REPO/target/release/workflow-cli" "\$@"
EOF
chmod 755 "$WRAPPER"

python3 - "$ROOT_REPO/config/config.toml" "$CONFIG" "$STATE" "$CACHE" \
    "$WRAPPER" "$ROOT/hardware.lock" "$PORT" <<'PY'
import re
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
state, cache, wrapper, lock, port = sys.argv[3:8]
source = source.replace("/home/kidpixel/workflow-rs/runtime", state)
source = source.replace("/mnt/cache", cache)
source = source.replace("/tmp/workflow-rs-hw.lock", lock)
source = source.replace(
    'workflow_cli = "/home/kidpixel/workflow-rs/target/release/workflow-cli"',
    f'workflow_cli = "{wrapper}"',
)
source = re.sub(r"(?m)^port = 5004$", f"port = {port}", source, count=1)


def patch_section(text: str, header: str, values: dict[str, str]) -> str:
    pattern = re.compile(
        rf"(?ms)^\[{re.escape(header)}\]\n(.*?)(?=^\[|\Z)"
    )
    match = pattern.search(text)
    if match is None:
        raise SystemExit(f"section [{header}] absente")
    body = match.group(1)
    for key, value in values.items():
        line = re.compile(rf"(?m)^{re.escape(key)} = .*$")
        body, count = line.subn(f"{key} = {value}", body, count=1)
        if count != 1:
            raise SystemExit(f"clé {key!r} absente de [{header}]")
    return text[: match.start(1)] + body + text[match.end(1) :]


source = patch_section(
    source,
    "webhook",
    {"enabled": "false", "dry_run": "true"},
)
source = patch_section(source, "steps.step8", {"timeout_s": "300"})
Path(sys.argv[2]).write_text(source, encoding="utf-8")
PY

if grep -qE '/mnt/cache|workflow-rs/runtime' "$CONFIG"; then
    fail "la configuration de test contient un chemin de production"
fi
if ! grep -q '^enabled = false$' "$CONFIG"; then
    fail "le webhook de test n'est pas désactivé"
fi
if ! grep -q '^port = '"$PORT"'$' "$CONFIG"; then
    fail "le port de test n'est pas $PORT"
fi

TOKEN="$(openssl rand -hex 32)"
SECRET="$(openssl rand -hex 32)"
umask 077
cat > "$ENV_FILE" <<EOF
WORKFLOW_SERVER_WORKER_TOKEN=$TOKEN
WORKFLOW_SERVER_SECRET_KEY=$SECRET
EOF
chmod 600 "$ENV_FILE"

log "fixture: $PROJECT"
log "config:  $CONFIG"
log "unit:    $UNIT_PATH"

# Préflight CLI : résout le manifeste et génère le driver sans EffectCraft.
"$ROOT_REPO/target/release/workflow-cli" --config "$CONFIG" \
    step8 run --logs-dir "$STATE/logs" --dry-run \
    >"$ROOT/step8-dry-run.log" 2>&1 \
    || fail "dry-run STEP8 en échec (voir $ROOT/step8-dry-run.log)"
grep -q 'MS_BATCH_TOTAL: 1' "$ROOT/step8-dry-run.log" \
    || fail "dry-run: un seul dossier attendu"

"$ROOT_REPO/scripts/install-systemd-user.sh" \
    --config "$CONFIG" \
    --env-file "$ENV_FILE" \
    --env-dest "$ENV_FILE" \
    --unit-name "$UNIT" \
    --url "http://127.0.0.1:$PORT" \
    >"$ROOT/install-unit.log" 2>&1 \
    || fail "installation de l'unité en échec"

systemd-analyze verify "$UNIT_PATH" >"$ROOT/systemd-analyze.log" 2>&1 \
    || fail "systemd-analyze verify en échec"
systemctl --user show "$UNIT.service" \
    -p Environment -p CacheDirectory -p ProtectSystem \
    -p NoNewPrivileges -p PrivateTmp -p ProtectHome \
    >"$ROOT/unit-show.txt"

# Sonde de permissions avec les mêmes propriétés que l'unité.
rm -f "$ROOT/perm-probe.txt"
systemd-run --user --wait --collect --unit="workflow-perm-probe" \
    --property=ProtectSystem=full \
    --property=NoNewPrivileges=yes \
    --property=PrivateTmp=false \
    --property=ProtectHome=no \
    /bin/sh -c '
        exec > "$1" 2>&1
        if touch /usr/local/bin/workflow-perm-probe 2>/dev/null; then
            echo usr_writable=FAIL
        else
            echo usr_readonly=OK
        fi
        touch /tmp/workflow-perm-probe && echo tmp_writable=OK
        touch "$HOME/workflow-perm-probe" && echo home_writable=OK
        rm -f /tmp/workflow-perm-probe "$HOME/workflow-perm-probe"
    ' sh "$ROOT/perm-probe.txt" \
    >/dev/null 2>&1 \
    || fail "sonde de permissions systemd en échec"
grep -q '^usr_readonly=OK$' "$ROOT/perm-probe.txt" \
    || fail "ProtectSystem=full ne rend pas /usr en lecture seule"
grep -q '^tmp_writable=OK$' "$ROOT/perm-probe.txt" \
    || fail "/tmp non inscriptible dans la sonde"
grep -q '^home_writable=OK$' "$ROOT/perm-probe.txt" \
    || fail "\$HOME non inscriptible dans la sonde"

START_EPOCH=$(date +%s)
systemctl --user start "$UNIT.service" \
    || fail "démarrage de l'unité en échec"
for _ in $(seq 1 40); do
    if curl -fs --max-time 2 -H "X-Worker-Token: $TOKEN" \
        "http://127.0.0.1:$PORT/api/ping" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done
curl -fs --max-time 2 -H "X-Worker-Token: $TOKEN" \
    "http://127.0.0.1:$PORT/api/ping" >/dev/null \
    || fail "serveur de test non joignable"

HTTP_CODE=$(curl -s -o "$ROOT/run-step8.json" -w '%{http_code}' \
    -X POST -H "X-Worker-Token: $TOKEN" \
    "http://127.0.0.1:$PORT/run/STEP8")
[[ "$HTTP_CODE" == "202" ]] || fail "POST /run/STEP8 -> HTTP $HTTP_CODE"

STATUS=""
for _ in $(seq 1 300); do
    curl -fs -H "X-Worker-Token: $TOKEN" \
        "http://127.0.0.1:$PORT/api/step_status/STEP8" \
        >"$ROOT/step8-status.json" 2>/dev/null || true
    STATUS=$(jq -r '.status // empty' "$ROOT/step8-status.json" 2>/dev/null || true)
    case "$STATUS" in
        completed|failed|stopped) break ;;
    esac
    sleep 1
done
[[ "$STATUS" == "completed" ]] || {
    journalctl --user -u "$UNIT.service" --since "@$START_EPOCH" --no-pager \
        >"$ROOT/journal.log" 2>&1 || true
    fail "STEP8 status=$STATUS (voir $ROOT/journal.log)"
}

REPORT=$(ls -1t "$STATE/logs/step8"/step8_run_*.json 2>/dev/null | head -1 || true)
[[ -n "$REPORT" ]] || fail "rapport STEP8 absent"
jq -e '
    .success == true and
    .mode == "headless" and
    .bridge_port == null and
    .app_launched == false and
    .timed_out == false and
    .result.ok == true and
    ((.result.errors // []) | length == 0)
' "$REPORT" >/dev/null \
    || fail "rapport STEP8 non conforme"

ECPROJ="$PROJECT/projets/short_2997_9x16.ecproj"
[[ -s "$ECPROJ" ]] || fail ".ecproj absent ou vide: $ECPROJ"
stat -c '%U:%G %a %s %n' "$ECPROJ" >"$ROOT/ecproj-stat.txt"
effectcraft-cli --project "$ECPROJ" info >"$ROOT/ecproj-info.txt" 2>&1 \
    || fail "effectcraft-cli ne peut pas ouvrir le .ecproj"
grep -q '1080' "$ROOT/ecproj-info.txt" || fail "largeur 1080 absente de info"
grep -q '1920' "$ROOT/ecproj-info.txt" || fail "hauteur 1920 absente de info"

jq -e --arg state "$STATE" '
    .folders | length == 1 and all(startswith($state))
' "$REPORT" >/dev/null || fail "le rapport référence un dossier hors fixture"
jq -e --arg state "$STATE" '
    .output_dir != null and (.output_dir | startswith($state))
' "$REPORT" >/dev/null || fail "output_dir hors fixture"

if find "$STATE" -user root -print -quit | grep -q .; then
    fail "fichier appartenant à root dans la fixture"
fi
if find /mnt/cache -maxdepth 2 -newermt "@$START_EPOCH" -print -quit 2>/dev/null | grep -q .; then
    fail "écriture détectée dans /mnt/cache"
fi
if find "$ROOT_REPO/runtime" -newermt "@$START_EPOCH" -print -quit 2>/dev/null | grep -q .; then
    fail "écriture détectée dans runtime/"
fi

grep -q 'CUDA_CACHE_PATH=/' "$ROOT/child-env.txt" \
    || fail "CUDA_CACHE_PATH non transmis au child"
grep -q 'WORKFLOW_SERVER_WORKER_TOKEN=<unset>' "$ROOT/child-env.txt" \
    || fail "WORKFLOW_SERVER_WORKER_TOKEN encore présent dans le child"
grep -q 'WORKFLOW_SERVER_SECRET_KEY=<unset>' "$ROOT/child-env.txt" \
    || fail "WORKFLOW_SERVER_SECRET_KEY encore présent dans le child"
grep -q 'FLASK_SECRET_KEY=<unset>' "$ROOT/child-env.txt" \
    || fail "FLASK_SECRET_KEY encore présent dans le child"
grep -q 'INTERNAL_WORKER_COMMS_TOKEN=<unset>' "$ROOT/child-env.txt" \
    || fail "INTERNAL_WORKER_COMMS_TOKEN encore présent dans le child"
grep -q 'MS_BATCH_RESULT: {"ok":true' "$STATE/logs/step_STEP8.log" \
    || fail "MS_BATCH_RESULT ok absent du journal STEP8"

if pgrep -x effectcraft-cli >/dev/null 2>&1; then
    fail "processus effectcraft-cli résiduel"
fi
if pgrep -x workflow-cli >/dev/null 2>&1; then
    fail "processus workflow-cli résiduel"
fi

journalctl --user -u "$UNIT.service" --since "@$START_EPOCH" --no-pager \
    >"$ROOT/journal.log" 2>&1 || true
if grep -q 'Permission denied' "$ROOT/journal.log"; then
    fail "Permission denied dans le journal de l'unité"
fi

log "STEP8_RESULT=OK"
log "REPORT=$REPORT"
log "ECPROJ=$ECPROJ"
log "ARTIFACTS=$ROOT"
