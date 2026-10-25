<p align="center">
  <img src="app/assets/brand/open-fader-logo-1024.png" width="160" alt="OpenFader logo" />
</p>

<h1 align="center">OpenFader</h1>

<p align="center">A Flutter app for controlling digital mixing consoles.</p>

OpenFader provides mixer controls, metering, and channel processing controls through a Flutter interface and an embedded Rust engine. The repository includes integrations for the Behringer X-Air series and Mackie DL32S, plus an offline simulator.

Available controls depend on the connected mixer and its driver. Choose **Work offline** on the Connections screen to explore the interface without hardware.

## Development

The Flutter app is in `app/`; the Rust engine, mixer drivers, simulator, command-line client, and server are in `crates/`. Build scripts are in `scripts/`.

Requirements: Flutter 3.47.7, Rust stable, and Node.js 24 for version checks. Native builds also require the target platform's build tools.

Install Flutter dependencies and check the app:

```sh
cd app
flutter pub get --enforce-lockfile
flutter analyze
```

Run the remaining checks from the repository root:

```sh
node scripts/sync-version.mjs --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Platform bundle builds use `python3 scripts/ci-build-app.py <platform>`, where `<platform>` is `ios`, `macos`, `android`, `linux`, or `windows`. Run the script on the host platform it requires.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
