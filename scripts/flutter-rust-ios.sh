#!/bin/bash
# Xcode build phase: compile and link the engine for this SDK and architecture.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$repo_root/target"
case "${PLATFORM_NAME}" in
  iphonesimulator) rust_suffix=apple-ios-sim ;;
  iphoneos) rust_suffix=apple-ios ;;
  *) echo "Unsupported Apple platform: $PLATFORM_NAME" >&2; exit 1 ;;
esac
profile=debug
cargo_profile=dev
if [[ "$CONFIGURATION" != Debug* ]]; then
  profile=release
  cargo_profile=release
fi
libraries=()
for arch in $ARCHS; do
  case "$arch" in
    arm64) target="aarch64-$rust_suffix" ;;
    x86_64) target=x86_64-apple-ios ;;
    *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
  esac
  # Prevent Xcode's iOS SDKROOT from leaking into host build scripts.
  env -u SDKROOT cargo build --locked --manifest-path "$repo_root/Cargo.toml" \
    -p openfader-flutter-bridge --target "$target" --profile "$cargo_profile"
  libraries+=("$CARGO_TARGET_DIR/$target/$profile/libopenfader_flutter_bridge.a")
done
xcrun lipo -create "${libraries[@]}" -output "$BUILT_PRODUCTS_DIR/libopenfader_flutter_bridge.a"
