#!/bin/bash
# Xcode build phase: build the real engine, bundle it before app signing.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$repo_root/target"
profile=debug
cargo_profile=dev
if [[ "$CONFIGURATION" != Debug* ]]; then
  profile=release
  cargo_profile=release
fi
mac_sdk="$(xcrun --sdk macosx --show-sdk-path)"
libraries=()
helpers=()
for arch in $ARCHS; do
  case "$arch" in
    arm64) target=aarch64-apple-darwin ;;
    x86_64) target=x86_64-apple-darwin ;;
    *) echo "Unsupported macOS architecture: $arch" >&2; exit 1 ;;
  esac
  env SDKROOT="$mac_sdk" MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --locked \
    --manifest-path "$repo_root/Cargo.toml" -p openfader-flutter-bridge \
    --target "$target" --profile "$cargo_profile"
  libraries+=("$CARGO_TARGET_DIR/$target/$profile/libopenfader_flutter_bridge.dylib")
  env SDKROOT="$mac_sdk" MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --locked \
    --manifest-path "$repo_root/Cargo.toml" -p openfader-audio --bin openfader-capture-helper \
    --target "$target" --profile "$cargo_profile"
  helpers+=("$CARGO_TARGET_DIR/$target/$profile/openfader-capture-helper")
done
frameworks="$TARGET_BUILD_DIR/$FRAMEWORKS_FOLDER_PATH"
mkdir -p "$frameworks"
library="$frameworks/libopenfader_flutter_bridge.dylib"
xcrun lipo -create "${libraries[@]}" -output "$library"
xcrun install_name_tool -id '@rpath/libopenfader_flutter_bridge.dylib' "$library"
/usr/bin/codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" "$library"

mkdir -p "$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/MacOS"
helper="$TARGET_BUILD_DIR/$CONTENTS_FOLDER_PATH/MacOS/openfader-capture-helper"
xcrun lipo -create "${helpers[@]}" -output "$helper"
if [ "$CODE_SIGNING_ALLOWED" = YES ]; then
  /usr/bin/codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" --options runtime --entitlements "$PROJECT_DIR/Runner/Host.entitlements" "$helper"
fi

# Build web before invoking the native Flutter build: nesting Flutter here
# would deadlock on its startup lock. CI prepares this bundle automatically.
web_source="$repo_root/app/build/web"
web_destination="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/web"
if [[ -f "$web_source/index.html" && -f "$web_source/flutter_bootstrap.js" ]]; then
  mkdir -p "$(dirname "$web_destination")"
  rm -rf "$web_destination"
  ditto "$web_source" "$web_destination"
else
  rm -rf "$web_destination"
  mkdir -p "$web_destination"
  echo "Browser hosting unavailable: run flutter build web --release --no-web-resources-cdn before building macOS."
fi
