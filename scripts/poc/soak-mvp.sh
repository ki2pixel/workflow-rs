#!/usr/bin/env bash
# Soak MVP J2 : 2 h de polling masqué, serveur et GUI isolés.
#
# Le script prépare une configuration sous /tmp/opencode, lance le runbook J4
# sans matrice, puis applique les gates stricts du MVP (delta réseau <= 0 Kio).
# Aucun accès à runtime/ ni à /mnt/cache.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT=""
DURATION=7200
INTERVAL=1.0
BASE_CONFIG=""
DISPLAY_VALUE="${DISPLAY:-:0}"

usage() {
    cat <<EOF
Usage: $0 [options]

Options:
  --out DIR          dossier des preuves (défaut: /tmp/opencode/mvp-j2/soak-<timestamp>)
  --duration S       durée du soak en secondes (défaut: 7200)
  --interval S       intervalle d'échantillonnage (défaut: 1.0)
  --base-config PATH configuration de base (défaut: générée sous OUT)
  --display DISPLAY  affichage X11 (défaut: $DISPLAY_VALUE)
  -h, --help         affiche cette aide

Le gate réseau strict est delta WebKitNetworkProcess <= 0 Kio. Le gate
contractuel J4 (<= 32 Mio) est aussi vérifié, mais ne suffit pas pour GO.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out) OUT="${2:?}"; shift 2 ;;
        --duration) DURATION="${2:?}"; shift 2 ;;
        --interval) INTERVAL="${2:?}"; shift 2 ;;
        --base-config) BASE_CONFIG="${2:?}"; shift 2 ;;
        --display) DISPLAY_VALUE="${2:?}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'option inconnue: %s\n' "$1" >&2; usage >&2; exit 2 ;;
    esac
done

if [[ -z "$OUT" ]]; then
    OUT="/tmp/opencode/mvp-j2/soak-$(date +%Y%m%d-%H%M%S)"
fi
case "$OUT" in
    /mnt/cache*|"$ROOT/runtime"*|"/")
        printf 'ERREUR: sortie interdite: %s\n' "$OUT" >&2
        exit 2
        ;;
esac
mkdir -p "$OUT"

fail() {
    printf 'ERREUR: %s\n' "$*" >&2
    exit 1
}

command -v systemd-inhibit >/dev/null 2>&1 || fail "systemd-inhibit absent"
command -v jq >/dev/null 2>&1 || fail "jq absent"
command -v python3 >/dev/null 2>&1 || fail "python3 absent"
for binary in workflow-server workflow-cli workflow-gui-tauri; do
    [[ -x "$ROOT/target/release/$binary" ]] || fail "binaire absent: $ROOT/target/release/$binary"
done

# Le soak doit être totalement isolé du serveur de production.
if systemctl --user is-active --quiet workflow-server.service 2>/dev/null; then
    fail "workflow-server.service est actif; arrêter l'unité avant le soak"
fi
if pgrep -x workflow-gui-tauri >/dev/null 2>&1; then
    fail "une instance workflow-gui-tauri est déjà active"
fi
if gdbus call --session --dest org.freedesktop.DBus \
    --object-path /org/freedesktop/DBus \
    --method org.freedesktop.DBus.GetNameOwner org.workflow.desktop \
    >/dev/null 2>&1; then
    fail "le nom org.workflow.desktop est déjà possédé"
fi
for port in 5003 5004 5005; do
    if ss -ltn 2>/dev/null | grep -q ":$port "; then
        fail "port $port déjà occupé"
    fi
done

# Vérifie que le frontend servi contient bien le correctif de polling.
grep -q 'PollingManager.js?v=20261025a' "$ROOT/assets/web/static/main.js" \
    || fail "main.js ne référence pas PollingManager.js?v=20261025a"
grep -q 'no-store' "$ROOT/crates/workflow-server/src/http.rs" \
    || fail "l'en-tête no-store des assets statiques est absent"

if [[ -z "$BASE_CONFIG" ]]; then
    BASE_CONFIG="$OUT/soak-base.toml"
    python3 - "$ROOT/config/config.toml" "$BASE_CONFIG" <<'PY'
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
source = source.replace(
    "/home/kidpixel/workflow-rs/runtime", "/tmp/opencode/poc-gui/state"
)
source = source.replace(
    "/mnt/cache", "/tmp/opencode/poc-gui/state/finalized"
)
source = source.replace(
    'workdir = "/home/kidpixel/workflow-rs"',
    'workdir = "/tmp/opencode/poc-gui"',
)
source = source.replace("port = 5004", "port = 5005", 1)
Path(sys.argv[2]).write_text(source, encoding="utf-8")
PY
fi

if grep -qE '/mnt/cache|workflow-rs/runtime' "$BASE_CONFIG"; then
    fail "la configuration de base contient un chemin de production"
fi
if ! grep -q '^port = 5005$' "$BASE_CONFIG"; then
    fail "la configuration de base doit utiliser le port 5005"
fi

printf 'soak_out=%s\n' "$OUT"
printf 'soak_duration_s=%s interval_s=%s\n' "$DURATION" "$INTERVAL"

set +e
systemd-inhibit --what=idle:sleep --mode=block \
    --who=mvp-j2-soak --why="soak MVP J2" \
    python3 "$ROOT/scripts/poc/j4-runbook.py" \
    --out "$OUT" \
    --base-config "$BASE_CONFIG" \
    --skip-matrix \
    --soak-seconds "$DURATION" \
    --soak-interval "$INTERVAL" \
    --abort-vram 3900 \
    --display "$DISPLAY_VALUE" \
    2>&1 | tee "$OUT/console.log"
RUN_RC=${PIPESTATUS[0]}
set -e

if [[ ! -f "$OUT/summary.json" ]]; then
    printf 'ERREUR: summary.json absent (runbook rc=%s)\n' "$RUN_RC" >&2
    exit "$RUN_RC"
fi

python3 - "$OUT" <<'PY'
import csv
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
summary = json.loads((out / "summary.json").read_text(encoding="utf-8"))
soak = summary.get("soak") or {}
cleanup = summary.get("cleanup") or {}
acceptance = soak.get("acceptance") or {}
deltas = soak.get("deltas_kib") or {}
slopes = soak.get("slopes_kib_per_min") or {}
vram = soak.get("vram") or {}
tray = soak.get("tray") or {}
logs = soak.get("logs_bytes") or {}
screenshot = soak.get("screenshot") or {}

rows = []
csv_path = out / "soak" / "monitor" / "endurance.csv"
if csv_path.is_file():
    with csv_path.open(newline="", encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))

def ratio_nonempty(field: str) -> float:
    if not rows:
        return 0.0
    return sum(1 for row in rows if (row.get(field) or "").strip()) / len(rows)

def single_pid(field: str) -> bool:
    if not rows:
        return False
    for row in rows:
        pids = [pid for pid in (row.get(field) or "").split("|") if pid.strip()]
        if len(pids) != 1:
            return False
    return True

def stable(field: str) -> bool:
    if not rows:
        return False
    values = {(row.get(field) or "").strip() for row in rows}
    return len(values) == 1 and "" not in values

def check(name: str, ok: bool, detail: str = "") -> bool:
    print(f"{'OK' if ok else 'FAIL':4} {name}{(' — ' + detail) if detail else ''}")
    return ok

def le(value, limit) -> bool:
    return isinstance(value, (int, float)) and value <= limit

def ge(value, limit) -> bool:
    return isinstance(value, (int, float)) and value >= limit

def lt(value, limit) -> bool:
    return isinstance(value, (int, float)) and value < limit

def gt(value, limit) -> bool:
    return isinstance(value, (int, float)) and value > limit

checks = [
    check("summary.acceptance_ok", summary.get("acceptance_ok") is True),
    check("soak.soak_ok", soak.get("soak_ok") is True),
    check("cleanup.cleanup_ok", cleanup.get("cleanup_ok") is True),
    check("soak.timed_out=false", soak.get("timed_out") is False),
    check(
        "échantillons",
        abs(int(soak.get("samples") or 0) - int(soak.get("expected_samples") or 0)) <= 5,
        f"{soak.get('samples')} / {soak.get('expected_samples')}",
    ),
    check("fenêtre cachée >= 0.99", ge((soak.get("window") or {}).get("hidden_ratio"), 0.99)),
    check("GUI vivant >= 0.999", ge(soak.get("gui_alive_ratio"), 0.999)),
    check("PSS complet >= 0.999", ge(soak.get("pss_complete_ratio"), 0.999)),
    check("dérive p95 <= 50 ms", le((soak.get("drift_ms") or {}).get("p95"), 50)),
    check("dérive max <= 250 ms", le((soak.get("drift_ms") or {}).get("max"), 250)),
    check("hôte delta <= 12 Mio", le(deltas.get("host"), 12 * 1024)),
    check("hôte pente <= 0.08 Mio/min", le(slopes.get("host"), 0.08 * 1024)),
    check("GUI RSS delta <= 32 Mio", le(deltas.get("gui"), 32 * 1024)),
    check("GUI PSS delta <= 32 Mio", le(deltas.get("gui_pss"), 32 * 1024)),
    check("GUI pente <= 0.267 Mio/min", le(slopes.get("gui"), (32.0 / 120.0) * 1024)),
    check("WebKitNetwork delta <= 0 Kio (strict MVP)", le(deltas.get("network"), 0),
          f"{deltas.get('network')} Kio"),
    check("WebKitNetwork gate J4 <= 32 Mio", le(deltas.get("network"), 32 * 1024)),
    check("WebKitWeb delta <= 32 Mio", le(deltas.get("web"), 32 * 1024)),
    check("serveur delta <= 12 Mio", le(deltas.get("server"), 12 * 1024)),
    check("MemAvailable >= -256 Mio", ge(deltas.get("mem_available"), -256 * 1024)),
    check("swap <= 16 Mio", le(deltas.get("swap_used"), 16 * 1024)),
    check("VRAM pic < 3900 MiB", lt(vram.get("peak_mib"), 3900)),
    check("VRAM libre min > 196 MiB", gt(vram.get("free_min_mib"), 196)),
    check("aucun OOM", acceptance.get("oom_free") is True),
    check("tray Idle", tray.get("final_state") == "Idle"),
    check("tray changements <= 20", le(tray.get("changes"), 20)),
    check("D-Bus tray stable", soak.get("dbus_stable") is True),
    check("aucun échec serveur", (soak.get("watcher") or {}).get("server_failures") == 0),
    check("logs hôte <= 16 Mio", le(logs.get("host"), 16 * 1024 * 1024)),
    check("logs serveur <= 16 Mio", le(logs.get("server"), 16 * 1024 * 1024)),
    check("CSV endurance <= 8 Mio", le(logs.get("endurance_csv"), 8 * 1024 * 1024)),
    check("capture finale non blanche", screenshot.get("ok") is True and screenshot.get("blank") is not True),
    check("network_pids présents >= 99 %", ratio_nonempty("network_pids") >= 0.99,
          f"{ratio_nonempty('network_pids'):.3f}"),
    check("un seul processus GUI hôte", single_pid("host_pids")),
    check("membres de session GUI stables", stable("gui_proc_count"),
          f"{sorted({(row.get('gui_proc_count') or '').strip() for row in rows})}"),
    check("token absent des artefacts", cleanup.get("token_absent") is True),
    check("ports libres", not cleanup.get("ports_busy")),
    check("aucun processus résiduel", not any((cleanup.get("residuals") or {}).values())),
    check("verrou matériel libéré", cleanup.get("hardware_lock_held") is False),
    check("GNOME inchangé", cleanup.get("gnome_unchanged") is True),
]

if not all(checks):
    print("SOAK_RESULT=FAIL")
    sys.exit(1)
print("SOAK_RESULT=OK")
PY
VALIDATION_RC=$?

if [[ "$RUN_RC" != "0" ]]; then
    printf 'SOAK_RUN_RC=%s\n' "$RUN_RC" >&2
    exit "$RUN_RC"
fi
if [[ "$VALIDATION_RC" != "0" ]]; then
    exit "$VALIDATION_RC"
fi

printf 'SOAK_OUT=%s\n' "$OUT"
