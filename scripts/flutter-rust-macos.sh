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
done
frameworks="$TARGET_BUILD_DIR/$FRAMEWORKS_FOLDER_PATH"
mkdir -p "$frameworks"
library="$frameworks/libopenfader_flutter_bridge.dylib"
xcrun lipo -create "${libraries[@]}" -output "$library"
xcrun install_name_tool -id '@rpath/libopenfader_flutter_bridge.dylib' "$library"
/usr/bin/codesign --force --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" "$library"
