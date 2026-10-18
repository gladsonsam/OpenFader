#!/usr/bin/env bash
# Dev loop against the REAL XR18 (192.168.50.123, FW 1.20 "Example Mixer").
#
#   ./scripts/dev-xr18.sh        start the relay (or show status if running)
#   ./scripts/dev-xr18.sh stop   stop the relay
#
# UI (hot-reload, already running — never started/stopped here):
#   http://127.0.0.1:5291/?server=127.0.0.1:8081
#   (?server= must match the relay port printed below.)
#
# KNOWN BLOCKER (2026-10-18): the current UI build sends
# {"cmd":"connect","driver":"sim"} on every connect
# (apps/web/src/ui/state/live.svelte.ts:135), which flips this relay from
# xair to sim. Until the UI stops sending that (native-feel agent's call),
# opening the dev URL flips the relay — re-run this script to restore xair.
# The desk itself is never written to (relay only polls it).
set -u
REPO="$(cd "$(dirname "$0")/.." && pwd)"
ADDR=192.168.50.123
BIN="$REPO/target/debug/openmix-serve"
LOG=/tmp/openmix-xr18-relay.log
PIDF=/tmp/openmix-xr18-relay.pid

port_free() { ! ss -ltn 2>/dev/null | grep -q ":$1 "; }

relay_driver() { curl -s -m 5 "http://127.0.0.1:$1/healthz" 2>/dev/null || echo "?"; }

do_stop() {
  if [ -f "$PIDF" ]; then
    pid="$(cat "$PIDF")"
    if kill "$pid" 2>/dev/null; then echo "stopped relay pid $pid"; else echo "relay pid $pid not running"; fi
    rm -f "$PIDF" "$PIDF.port"
  else
    echo "no relay pid file ($PIDF) — nothing to stop"
  fi
}

if [ "${1:-}" = "stop" ]; then do_stop; exit 0; fi

# Never touch :8080 (other agent's sim dev server).
if [ -f "$PIDF" ] && kill -0 "$(cat "$PIDF")" 2>/dev/null; then
  PORT="$(cat "$PIDF.port" 2>/dev/null || echo 8081)"
  echo "relay already running (pid $(cat "$PIDF"))"
  echo "healthz: $(relay_driver "$PORT")"
  echo "dev URL: http://127.0.0.1:5291/?server=127.0.0.1:$PORT"
  exit 0
fi

PORT=8081
while ! port_free "$PORT"; do PORT=$((PORT + 1)); done

"$BIN" --port "$PORT" --driver xair --addr "$ADDR" >"$LOG" 2>&1 < /dev/null &
# detach fully (script may exit; relay stays)
disown 2>/dev/null || true
echo $! > "$PIDF"
echo "$PORT" > "$PIDF.port"
sleep 3
echo "relay pid $(cat "$PIDF") -> http://127.0.0.1:$PORT (log: $LOG)"
echo "healthz: $(relay_driver "$PORT")"
echo "desk:    $(timeout 30 "$REPO/target/debug/openmix-cli" --driver xair --addr "$ADDR" list 2>/dev/null | head -3 | tr '\n' ';')"
echo "dev URL (hot-reload UI, live desk): http://127.0.0.1:5291/?server=127.0.0.1:$PORT"
