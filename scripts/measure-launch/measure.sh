#!/usr/bin/env bash
# Deterministic launch measurement for openmix on iOS simulator.
#
#   ./scripts/measure-launch/measure.sh soft [runs=3]   # warm sim, terminate+launch
#   ./scripts/measure-launch/measure.sh cold [runs=1]   # reboot sim, settle, launch
#
# Metric: wall-clock from `simctl launch` to first mixer screenshot.
# Detector: classify.py (green CH buttons/meters). Prints one JSON line per run.
# Compare runs across worktrees/commits to see real improvements.
#
# Cold mode separates SIM-BOOT time from APP-LAUNCH time: after boot it waits
# for the home screen (settle), so t_mixer is pure app cost.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UDID="${SIM_UDID:-2CE6A81C-2751-4B20-9B1A-416817A2CAAE}"
BUNDLE_ID="com.openmix.app"
MODE="${1:-soft}"
RUNS="${2:-3}"
TIMEOUT=90
OUTDIR="${OUTDIR:-/tmp/launchframes}"

mkdir -p "$OUTDIR"

wait_for_home() {
  # Settle: home screen = neither white(boot) nor black, no mixer, stable twice.
  local stable=0
  for _ in $(seq 1 60); do
    local f="$OUTDIR/settle.png"
    xcrun simctl io "$UDID" screenshot "$f" >/dev/null 2>&1 || { sleep 2; continue; }
    local screen
    screen=$(python3 "$ROOT/scripts/measure-launch/classify.py" "$f" | python3 -c 'import json,sys; print(json.load(sys.stdin)["screen"])')
    if [ "$screen" = "other" ]; then
      stable=$((stable + 1))
      [ "$stable" -ge 2 ] && return 0
    else
      stable=0
    fi
    sleep 2
  done
  echo "WARN: settle timeout, measuring anyway" >&2
}

one_run() {
  local tag="$1"
  xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true
  sleep 1
  local start end f screen green elapsed=0 mixer_at="" first_paint_at=""
  start=$(python3 -c 'import time; print(time.monotonic())')
  xcrun simctl launch "$UDID" "$BUNDLE_ID" >/dev/null
  local i=0
  while python3 -c "import sys; sys.exit(0 if $elapsed < $TIMEOUT else 1)"; do
    i=$((i + 1))
    f="$OUTDIR/$tag-$i.png"
    xcrun simctl io "$UDID" screenshot "$f" >/dev/null 2>&1 || { sleep 1; elapsed=$((elapsed + 1)); continue; }
    local json
    json=$(python3 "$ROOT/scripts/measure-launch/classify.py" "$f")
    screen=$(echo "$json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["screen"])')
    end=$(python3 -c 'import time; print(time.monotonic())')
    elapsed=$(python3 -c "print(round($end - $start, 2))")
    if [ -z "$first_paint_at" ] && { [ "$screen" = "black" ] || [ "$screen" = "mixer" ]; }; then
      # black = storyboard dismissed / webview surface live (first paint soon)
      first_paint_at="$elapsed"
    fi
    if [ "$screen" = "mixer" ]; then
      mixer_at="$elapsed"
      mv "$f" "$OUTDIR/$tag-MIXER-${elapsed}s.png"
      break
    fi
    sleep 1
  done
  python3 -c "import json; print(json.dumps({'mode':'$MODE','run':'$tag','t_first_surface_s':${first_paint_at:-null},'t_mixer_s':${mixer_at:-null}}))"
}

if [ "$MODE" = "cold" ]; then
  xcrun simctl shutdown "$UDID" >/dev/null 2>&1 || true
  sleep 2
  xcrun simctl boot "$UDID"
  xcrun simctl bootstatus "$UDID" -b >/dev/null
  wait_for_home
fi

for n in $(seq 1 "$RUNS"); do
  one_run "$MODE-$n"
done
