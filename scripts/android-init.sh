#!/usr/bin/env bash
# Re-apply openmix Android manifest permissions after `tauri android init`.
#
# `src-tauri/gen/` is git-ignored, so every fresh `android init` (CI, new
# machine) wipes these. Run this script after init:
#
#   ./scripts/android-init.sh
#
# What it does (idempotent — safe to re-run):
#   - inserts INTERNET / ACCESS_NETWORK_STATE / ACCESS_WIFI_STATE
#     <uses-permission> lines into
#     src-tauri/gen/android/app/src/main/AndroidManifest.xml when missing
#   - warns if android:usesCleartextTraffic="true" is set outside debug
#     (must stay debug-only; release builds talk to the desk over the LAN,
#     never cleartext internet)
#
# Deliberately absent (do NOT add here):
#   CHANGE_WIFI_MULTICAST_STATE — discovery is UDP broadcast with unicast
#   replies, no multicast lock needed.
#   ACCESS_FINE_LOCATION — no SSID scans.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/src-tauri/gen/android/app/src/main/AndroidManifest.xml"

if [[ ! -f "$MANIFEST" ]]; then
  echo "android-init: $MANIFEST not found." >&2
  echo "android-init: run \`npx tauri android init\` first (on a tooled machine), then re-run this script." >&2
  exit 1
fi

PERMS=(
  "android.permission.INTERNET"
  "android.permission.ACCESS_NETWORK_STATE"
  "android.permission.ACCESS_WIFI_STATE"
)

added=0
for perm in "${PERMS[@]}"; do
  if grep -q "$perm" "$MANIFEST"; then
    echo "android-init: present: $perm"
  else
    python3 - "$MANIFEST" "$perm" <<'EOF'
import sys
path, perm = sys.argv[1], sys.argv[2]
with open(path, encoding="utf-8") as f:
    text = f.read()
line = f'    <uses-permission android:name="{perm}" />\n'
anchor = "<application"
assert anchor in text, "no <application> element in AndroidManifest.xml"
text = text.replace(anchor, line + "    " + anchor, 1)
with open(path, "w", encoding="utf-8") as f:
    f.write(text)
EOF
    echo "android-init: added: $perm"
    added=$((added + 1))
  fi
done

if grep -q 'usesCleartextTraffic="true"' "$MANIFEST"; then
  echo "android-init: WARNING: android:usesCleartextTraffic=\"true\" is set — must stay debug-only." >&2
else
  echo "android-init: cleartext check ok (no release usesCleartextTraffic=true)."
fi

if [[ "$added" -gt 0 ]]; then
  echo "android-init: done ($added permission(s) added)."
else
  echo "android-init: done (already up to date)."
fi
