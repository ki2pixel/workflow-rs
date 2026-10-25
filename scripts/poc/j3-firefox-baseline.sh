#!/usr/bin/env bash
set -euo pipefail

OUT="${1:?usage: j3-firefox-baseline.sh <out-dir>}"
BASELINE_SECONDS="${BASELINE_SECONDS:-60}"
SETTLE_SECONDS="${SETTLE_SECONDS:-15}"
SERVER_URL="${SERVER_URL:-http://127.0.0.1:5005/}"
SNAP_HOME="/home/kidpixel/snap/firefox/common"
PROFILE_ROOT="$SNAP_HOME/poc-j3"
mkdir -p "$OUT" "$PROFILE_ROOT"
PROFILE="$(mktemp -d "$PROFILE_ROOT/profile.XXXXXX")"
WID=""
FF_PID=""
FF_SID=""
LAUNCH_PID=""

cleanup() {
  if [[ -n "$WID" ]]; then
    wmctrl -i -c "$WID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$FF_SID" ]]; then
    kill -TERM -- "-$FF_SID" >/dev/null 2>&1 || true
    sleep 1
    kill -KILL -- "-$FF_SID" >/dev/null 2>&1 || true
  elif [[ -n "$FF_PID" ]]; then
    kill -TERM "$FF_PID" >/dev/null 2>&1 || true
  elif [[ -n "$LAUNCH_PID" ]]; then
    kill -TERM "$LAUNCH_PID" >/dev/null 2>&1 || true
  fi
  sleep 0.5
  rm -rf "$PROFILE" >/dev/null 2>&1 || true
  mkdir -p "$OUT"
  if [[ ! -d "$PROFILE" ]]; then
    PROFILE_REMOVED=true
  else
    PROFILE_REMOVED=false
  fi
  PROFILE_REMOVED="$PROFILE_REMOVED" FF_PID="$FF_PID" FF_SID="$FF_SID" WID="$WID" \
    python3 - "$OUT/firefox-cleanup.json" <<'PY'
import json
import os
import sys
import time
import psutil

path = sys.argv[1]
leftovers = []
for proc in psutil.process_iter(["pid", "name", "cmdline"]):
    try:
        name = proc.info["name"] or ""
        cmdline_list = proc.info["cmdline"] or []
        first = os.path.basename(cmdline_list[0]) if cmdline_list else ""
        if name in ("firefox", "firefox-bin") or first in ("firefox", "firefox-bin"):
            leftovers.append(
                {
                    "pid": proc.info["pid"],
                    "name": name,
                    "cmd": " ".join(cmdline_list)[:300],
                }
            )
    except psutil.Error:
        pass
payload = {
    "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "wid": os.environ.get("WID", ""),
    "ff_pid": os.environ.get("FF_PID", ""),
    "ff_sid": os.environ.get("FF_SID", ""),
    "profile_removed": os.environ.get("PROFILE_REMOVED") == "true",
    "firefox_leftovers": leftovers,
    "ok": os.environ.get("PROFILE_REMOVED") == "true" and not leftovers,
}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, ensure_ascii=False)
    handle.write("\n")
PY
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

if pgrep -x firefox >/dev/null 2>&1; then
  echo "ERREUR: fermer toutes les instances Firefox avant la baseline" >&2
  exit 2
fi

mkdir -p "$OUT" "$PROFILE_ROOT"
: > "$OUT/firefox.log"

cat > "$PROFILE/user.js" <<EOF
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.laterrun.enabled", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("browser.startup.homepage_override_url", "");
user_pref("browser.startup.page", 1);
user_pref("browser.startup.homepage", "$SERVER_URL");
user_pref("browser.newtabpage.enabled", false);
user_pref("browser.newtab.preload", false);
user_pref("browser.newtabpage.activity-stream.feeds.topsites", false);
user_pref("browser.newtabpage.activity-stream.feeds.section.topstories", false);
user_pref("browser.newtabpage.activity-stream.feeds.section.highlights", false);
user_pref("browser.newtabpage.activity-stream.feeds.snippets", false);
user_pref("browser.newtabpage.activity-stream.feeds.system.topstories", false);
user_pref("browser.newtabpage.activity-stream.showSponsored", false);
user_pref("browser.newtabpage.activity-stream.showSponsoredTopSites", false);
user_pref("browser.newtabpage.activity-stream.default.sites", "");
user_pref("browser.newtabpage.activity-stream.telemetry", false);
user_pref("browser.newtabpage.activity-stream.asrouter.userprefs.cfr.addons", false);
user_pref("browser.newtabpage.activity-stream.asrouter.userprefs.cfr.features", false);
user_pref("browser.messaging-system.whatsNewPanel.enabled", false);
user_pref("browser.sessionstore.resume_from_crash", false);
user_pref("browser.sessionstore.resume_session_once", false);
user_pref("browser.sessionstore.restore_on_demand", false);
user_pref("browser.tabs.warnOnClose", false);
user_pref("browser.tabs.warnOnOpen", false);
user_pref("browser.ping-centre.telemetry", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("toolkit.telemetry.unified", false);
user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);
user_pref("app.update.auto", false);
user_pref("app.update.enabled", false);
user_pref("extensions.update.enabled", false);
user_pref("extensions.autoDisableScopes", 15);
user_pref("browser.search.update", false);
user_pref("browser.search.suggest.enabled", false);
user_pref("browser.urlbar.suggest.searches", false);
user_pref("network.prefetch-next", false);
user_pref("network.dns.disablePrefetch", true);
user_pref("network.predictor.enabled", false);
EOF
cp "$PROFILE/user.js" "$OUT/firefox-user.js"

setsid env MOZ_NO_REMOTE=1 MOZ_ENABLE_WAYLAND=0 GDK_BACKEND=x11 \
  firefox --new-instance --profile "$PROFILE" \
  >"$OUT/firefox.log" 2>&1 &
LAUNCH_PID=$!

for _ in $(seq 1 150); do
  mapfile -t WIDS < <(wmctrl -lpx 2>/dev/null | awk '$4 ~ /firefox/ {print $1}')
  if (( ${#WIDS[@]} == 1 )); then
    WID="${WIDS[0]}"
    break
  fi
  sleep 0.2
done
if [[ -z "$WID" ]]; then
  echo "ERREUR: fenêtre Firefox absente ou multiple (voir $OUT/firefox.log)" >&2
  exit 3
fi

FF_PID="$(xprop -id "$WID" _NET_WM_PID 2>/dev/null | awk '{print $3}')"
if [[ -z "$FF_PID" ]]; then
  echo "ERREUR: _NET_WM_PID absent" >&2
  exit 4
fi
FF_SID="$(ps -o sid= -p "$FF_PID" | tr -d ' ')"

wmctrl -i -r "$WID" -e 0,240,109,1440,900 >/dev/null 2>&1 || {
  xdotool windowsize "$WID" 1440 900
  xdotool windowmove "$WID" 240 109
}
wmctrl -i -a "$WID" >/dev/null 2>&1 || true

sleep "$SETTLE_SECONDS"
TITLE="$(xdotool getwindowname "$WID" 2>/dev/null || true)"
if [[ "$TITLE" == *"New Tab"* ]]; then
  echo "ERREUR: onglet New Tab détecté: $TITLE" >&2
  exit 5
fi
import -window "$WID" "$OUT/firefox-final.png"
identify -format 'geometry=%wx%h mean=%[fx:mean] stddev=%[fx:standard_deviation] colors=%k\n' \
  "$OUT/firefox-final.png" | tee "$OUT/firefox-final.txt"

python3 - "$OUT/firefox-final.png" "$OUT/firefox-window.json" "$WID" "$FF_PID" "$FF_SID" "$TITLE" <<'PY'
import json
import math
import subprocess
import sys

png, path, wid, pid, sid, title = sys.argv[1:7]
proc = subprocess.run(
    ["identify", "-format", "%w %h %[fx:mean] %[fx:standard_deviation] %k", png],
    capture_output=True,
    text=True,
    check=False,
)
if proc.returncode != 0:
    raise SystemExit(f"identify: {proc.stderr}")
parts = proc.stdout.strip().split()
width, height = int(parts[0]), int(parts[1])
mean, stddev, colors = float(parts[2]), float(parts[3]), int(parts[4])
blank = ""
if math.isnan(stddev) or colors <= 1:
    blank = "solid"
elif mean > 0.95 and stddev < 0.02:
    blank = "white"
elif mean < 0.05 and stddev < 0.02:
    blank = "black"
payload = {
    "wid": wid,
    "pid": int(pid),
    "sid": int(sid),
    "title": title,
    "geometry": {"width": width, "height": height},
    "screenshot": {
        "mean": mean,
        "stddev": None if math.isnan(stddev) else stddev,
        "colors": colors,
        "blank": blank,
    },
}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2, ensure_ascii=False)
    handle.write("\n")
if blank:
    raise SystemExit(f"capture Firefox invalide: {blank}")
PY

python3 scripts/poc/j3-monitor.py \
  --role firefox \
  --root-pid "$FF_PID" \
  --profile-path "$PROFILE" \
  --phase firefox-baseline \
  --duration "$BASELINE_SECONDS" \
  --interval 1 \
  --out "$OUT/monitor"
