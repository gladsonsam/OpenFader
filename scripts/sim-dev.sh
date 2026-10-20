#!/usr/bin/env bash
# Rebuild the iOS app (embedded production frontend) and put it on a booted
# simulator. This is the ONLY reliable way to see frontend edits in the
# simulator build — see docs/sim-verify.md for why.
#
#   ./scripts/sim-dev.sh                 # web build + rust embed + xcode + install + launch
#   ./scripts/sim-dev.sh --web-only      # just `npm run build` (fast sanity)
#   SIM_UDID=... ./scripts/sim-dev.sh    # pick a simulator (default: first booted)
#
# Steps, in order (each one is required — skipping any leaves a stale app):
#   1. apps/web build           -> apps/web/dist
#   2. cargo build (ios-sim)     -> embeds dist into the binary via generate_context!
#   3. xcodebuild                -> assembles openmix.app around that dylib
#   4. simctl install + launch   -> replaces the installed app
#
# The Xcode project's "Build Rust Code" phase is a no-op shellScript, so step 2
# never happens automatically: without it the app keeps serving the previously
# embedded bundle and every measurement is of old code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
PROJ="$ROOT/src-tauri/gen/apple/openmix-app.xcodeproj"
SCHEME="openmix-app_iOS"
DERIVED="$ROOT/src-tauri/gen/apple/build"
BUNDLE_ID="com.openmix.app"

WEB_ONLY=0
[ "${1:-}" = "--web-only" ] && WEB_ONLY=1

step() { printf '\n== %s\n' "$1"; }

step "web build"
npm --prefix "$ROOT/apps/web" run build | tail -2

if [ "$WEB_ONLY" = "1" ]; then
  echo "web-only: done"
  exit 0
fi

step "rust build (re-embeds apps/web/dist)"
cargo build --manifest-path "$ROOT/src-tauri/Cargo.toml" --target aarch64-apple-ios-sim | tail -2

step "gen/apple self-heal (splash + staticlib symlinks)"
# LaunchScreen: gen/apple is gitignored and regenerated stock by
# `tauri ios init`, so re-apply the committed branded template on every
# build (cmp-guarded to keep warm rebuilds fast). See docs/sim-verify.md.
SPLASH_SRC="$ROOT/scripts/ios/LaunchScreen.storyboard"
SPLASH_DST="$ROOT/src-tauri/gen/apple/LaunchScreen.storyboard"
if ! cmp -s "$SPLASH_SRC" "$SPLASH_DST"; then
  cp "$SPLASH_SRC" "$SPLASH_DST"
  echo "splash re-applied"
fi
# Externals/libapp.a is a symlink into a cargo target dir. Worktrees get
# gen/apple via rsync from the main checkout, which copies the ABSOLUTE
# symlink — so a worktree xcodebuild would silently link (and ship) the
# main checkout's staticlib. Repoint at this checkout's own target when
# the link aims elsewhere; a no-op where it already points at itself.
for l in "$ROOT"/src-tauri/gen/apple/Externals/*/debug/libapp.a \
         "$ROOT"/src-tauri/gen/apple/Externals/*/release/libapp.a; do
  [ -L "$l" ] || continue
  case "$l" in
    */arm64/*) triple=aarch64-apple-ios-sim ;;
    */x86_64/*) triple=x86_64-apple-ios ;;
    *) continue ;;
  esac
  case "$l" in
    */debug/*) cfg=debug ;; *) cfg=release ;;
  esac
  want="$ROOT/src-tauri/target/$triple/$cfg/libopenmix_app.a"
  cur="$(readlink "$l")"
  if [ "$cur" != "$want" ]; then
    ln -sfn "$want" "$l"
    echo "repointed $l -> $want (was $cur)"
  fi
done

step "xcodebuild"
xcodebuild -project "$PROJ" -scheme "$SCHEME" -configuration debug \
  -sdk iphonesimulator -derivedDataPath "$DERIVED" build >/tmp/sim-dev-xcodebuild.log 2>&1 \
  || { tail -25 /tmp/sim-dev-xcodebuild.log; exit 1; }
echo "built: $DERIVED/Build/Products/debug-iphonesimulator/openmix.app"

UDID="${SIM_UDID:-$(xcrun simctl list devices booted -j | python3 -c 'import json,sys; d=json.load(sys.stdin)["devices"]; print(next(x["udid"] for v in d.values() for x in v if x["state"]=="Booted"))')}"
step "install + launch on $UDID"
xcrun simctl terminate "$UDID" "$BUNDLE_ID" >/dev/null 2>&1 || true
xcrun simctl install "$UDID" "$DERIVED/Build/Products/debug-iphonesimulator/openmix.app"
xcrun simctl launch "$UDID" "$BUNDLE_ID" >/dev/null
echo "launched $BUNDLE_ID"
