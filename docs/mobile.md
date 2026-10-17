# Mobile prep (Tauri Android / iOS)

Status: **config-only prep committed; native projects NOT generated on this
machine** (toolchains absent — see environment record below). No signing is
configured. Desktop builds are unaffected.

## 1. Environment record (2026-10-17, this machine)

| Item | Result |
| --- | --- |
| OS | Debian 13 (trixie), Linux 6.12 x86_64 |
| Node / npm | v24.19.0 / 11.17.0 |
| Tauri CLI | 2.12.3 (via `npx -y @tauri-apps/cli`) |
| Rust | rustc/cargo 1.99.0 |
| Java | 21.0.12.1 (present — usable for Android builds later) |
| `ANDROID_HOME` / `ANDROID_SDK_ROOT` | **unset** |
| `adb` / `sdkmanager` / `avdmanager` / `gradle` | **not found** |
| `~/Android`, `/usr/lib/android-sdk`, `/opt/android*` | **absent** |
| NDK | **absent** |
| Xcode / `xcodebuild` | **N/A (Linux)** |
| `tauri ios` subcommand | **not available** on this platform (`unrecognized subcommand 'ios'` — iOS init requires macOS) |
| Installed Rust targets | `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` only (no `*-linux-android`, no Apple targets) |

Because of the above, `tauri android init` / `tauri ios init` were
**deliberately not run** here: they would fail (no SDK/NDK, no Xcode) and a
half-generated `src-tauri/gen/` tree would only add noise. The owner runs them
once per §3 on a machine with the toolchains.

## 2. What this commit adds (and why desktop still builds)

- `src-tauri/tauri.conf.json` — explicit `bundle.android.minSdkVersion: 24`
  and `bundle.iOS.minimumSystemVersion: "15.0"`. These are the Tauri schema
  defaults, so desktop output is byte-identical in behavior; they pin the
  mobile floor and give the owner a place to set `versionCode` /
  `developmentTeam` later.
- `src-tauri/capabilities/mobile.json` (new) — platform-scoped capability
  (`"platforms": ["Android", "iOS"]`, strings verified against the
  `tauri-utils` `Target` enum) re-granting `core:default` to the `main`
  window. Desktop ignores it (platform filter); mobile gets the invoke grant
  plus a reserved slot for future mobile-only plugin permissions.
- No Rust code changed. Mixer I/O is raw UDP sockets in `openmix-core` /
  driver crates, not a Tauri plugin, so **no extra ACL permission strings
  are required** — the OS-level network permissions live in the generated
  native projects (§5), which do not exist until init.

Verification: `python3 -m json.tool` on both JSON files (syntax valid);
`cargo check -p openmix-core` still passes (`src-tauri` is in workspace
`exclude`, so the desktop sysdeps blocker — `libdbus-1` — is untouched).

## 3. One-time toolchain setup (owner-side)

### Android (Linux or macOS)

```sh
# 1. JDK 17+ (already present here: 21) + Android Studio (includes SDK),
#    or command-line tools only:
#    https://developer.android.com/studio#command-line-tools-only
export ANDROID_HOME="$HOME/Android/Sdk"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH"

sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0" "ndk;26.1.10909125"

# 2. Rust targets ( Tauri needs all four for `android dev`/`build` )
rustup target add aarch64-linux-android armv7-linux-androideabi \
  i686-linux-android x86_64-linux-android

# 3. Generate the native project (config only, no signing)
cd src-tauri
cargo tauri android init
git status  # review src-tauri/gen/android (generated; keep or gitignore per repo policy)
```

### iOS (macOS only)

1. Install Xcode from the App Store, open it once, accept the license,
   install additional components.
2. `xcode-select --install` (command-line tools).
3. Rust targets: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios`.
4. Enroll in the Apple Developer Program (needed for device runs beyond the
   free 7-day provisioning, and mandatory for TestFlight/App Store).
5. `cd src-tauri && cargo tauri ios init`.

## 4. Dev-run on a real device (mixer LAN)

The touch-fader latency test only means something on hardware, on the same
LAN as the mixer.

```sh
cd src-tauri
# Android: enable USB debugging on the phone, plug in, accept the RSA prompt
adb devices            # device must show as `device`, not `unauthorized`
cargo tauri android dev
# iOS (macOS): plug in, trust the computer, select team if prompted
cargo tauri ios dev
```

Notes:

- The dev server (`devUrl`, currently `http://localhost:1420`) must be
  reachable **from the phone**. Easiest real-world setup: laptop and phone
  both on the mixer LAN (the same network the desk is on), `beforeDevCommand`
  bound to `0.0.0.0` — do not ship that binding to production.
- If USB-only: `adb reverse tcp:1420 tcp:1420` so the phone's `localhost:1420`
  reaches the laptop.
- In the app, `connect` with driver `xair`/`dl32s` and the mixer's LAN IP
  (e.g. `192.168.1.10`) — same as desktop; the UDP socket is opened from Rust,
  not from the webview.
- Alternative with zero native setup: run `openmix-serve` on a laptop on the
  mixer LAN and open the PWA (`apps/web`, installable) in the phone browser.
  That path needs no store, no signing, no manifest edits.

## 5. LAN permission notes (mixer UDP)

The app talks to the desk over **unicast UDP to an explicit IP**
(X-Air OSC on 10032/10024-style ports; DL32S Master Fader protocol). The
sockets are opened in Rust, so the Tauri ACL is not involved — but the OS
still requires these declarations in the **generated** projects (they only
exist after §3; nothing to edit in this repo yet).

### Android — `src-tauri/gen/android/app/src/main/AndroidManifest.xml`

Verify (add if missing) inside `<manifest>`:

```xml
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
```

- `INTERNET` (normal, auto-granted) is **mandatory** — without it every
  socket, including Rust UDP, fails.
- `ACCESS_NETWORK_STATE` (normal) is recommended so the app can detect
  LAN loss vs. mixer silence.
- Do **not** need: `CHANGE_WIFI_MULTICAST_STATE` (multicast lock — we use
  unicast to a known IP, no discovery multicast), `ACCESS_FINE_LOCATION`
  (only for Wi-Fi SSID/BSSID scans, which we do not do).
- Cleartext HTTP (`usesCleartextTraffic`) only matters for the dev-server
  URL during `android dev`; release builds load bundled assets. If `dev`
  shows a blank page on Android 9+, check the generated manifest's
  `<application android:usesCleartextTraffic="true">` for the debug build.

### iOS — `Info.plist` + entitlement

After `ios init`, merge via `bundle.iOS.infoPlist` (or `src-tauri/Info.ios.plist`):

```xml
<key>NSLocalNetworkUsageDescription</key>
<string>openmix connects to your mixer (X-Air / DL32S) on the local network.</string>
```

- iOS 14+ shows the Local Network prompt on first UDP use; the string above
  is what the user sees. Without the key, local sockets fail silently.
- **No multicast entitlement needed**: `com.apple.developer.networking.multicast`
  is only for multicast/bonjour discovery; we connect to explicit unicast IPs.
- `NSBonjourServices` only if mDNS discovery is added later (not planned).

## 6. Signing checklist (store releases)

### Google Play

- [ ] Create an upload keystore once: `keytool -genkeypair -alias openmix -keyalg RSA -keysize 4096 -validity 9125 -keystore openmix-upload.jks`. Back it up offline; losing it means a new app listing.
- [ ] Enroll in **Play App Signing** (Google holds the final signing key).
- [ ] Add `signingConfigs` to the generated Gradle project
      (`src-tauri/gen/android/app/build.gradle`); never commit the `.jks` or
      passwords — pass via env/CI secrets.
- [ ] `cargo tauri android build` → upload the `.aab` (not APK) to Play.
- [ ] First release: internal testing track on a real mixer LAN before production.

### App Store

- [ ] Apple Developer Program membership; create App ID `com.openmix.app`.
- [ ] Set `bundle.iOS.developmentTeam` (or `APPLE_DEVELOPMENT_TEAM`) to the Team ID.
- [ ] Xcode automatic signing for debug; manual/distribution certificate + App Store
      provisioning profile for release (`cargo tauri ios build --export-method app-store`).
- [ ] Bump `version` in `tauri.conf.json` per release; optional
      `bundle.iOS.bundleVersion` for the build number.
- [ ] TestFlight first (external testers need only an email), then App Review.
      Note the Local Network prompt (§5) in the review notes + a demo video if
      review asks how the app is used without hardware.

## 7. Deferred to owner (explicitly not done here)

1. Run `android init` / `ios init` on a tooled machine.
2. Paste the §5 manifest/plist snippets into the generated projects and
   smoke-test `connect` → faders → meters against a real desk.
3. Set up signing (§6) when a store release is actually planned.
