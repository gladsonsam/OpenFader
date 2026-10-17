# Desktop build (Linux)

Date: 2026-10-17 (UTC)
Machine: Linux (Debian 13 / trixie family, x86_64)
Toolchain: cargo 1.99.0, node v24.19.0, `@tauri-apps/cli@2` via npx

## Exact command

Run from `src-tauri/`:

```sh
npx -y @tauri-apps/cli@2 build
```

- First attempt: succeeded, no retry needed.
- Full build log: `/var/tmp/t3tmp-gladson/tauri-build.log` (ends with `EXIT:0`)
- Release compile time: ~2m 00s (`Finished release profile [optimized]`)
- `src-tauri/` fixes required: none — `cargo check`-clean tree built as-is.
- Extra system packages installed via apt: none — WebKitGTK (`libwebkit2gtk-4.1-dev`),
  `libjavascriptcoregtk-4.1-dev`, `libsoup-3.0-dev`, `librsvg2-dev`, and
  Ayatana appindicator libs were already present.
- Frontend: used prebuilt `../apps/web/dist` as-is
  (`tauri.conf.json`: `frontendDist: ../apps/web/dist`, no `beforeBuildCommand`).

## Result: SUCCESS

Raw binary:

| Path | Size (bytes) | Size (human) |
| --- | --- | --- |
| `src-tauri/target/release/openmix-app` | 13901176 | ~13.3 MiB |

Bundles (`src-tauri/target/release/bundle/`):

| Path | Size (bytes) | Size (human) | sha256 |
| --- | --- | --- | --- |
| `src-tauri/target/release/bundle/deb/openmix_0.1.0_amd64.deb` | 3446020 | 3.29 MiB | `cbb460752bd0212e97c530b8ffc8e45da45eb3782136399911bdc1612910b25c` |
| `src-tauri/target/release/bundle/rpm/openmix-0.1.0-1.x86_64.rpm` | 3450019 | 3.29 MiB | `43e5c4ff5cbc51c1fbccb839fa66f871ba1e6b53b650e12238b5100d204314ea` |
| `src-tauri/target/release/bundle/appimage/openmix_0.1.0_amd64.AppImage` | 111675896 | 106.50 MiB | `cd26d0633151cf07aa39a3dc6a5dd9ed5b1968f61f6abb74744bc0f2ba243af3` |

(Tauri-reported sizes: deb 3.29 MiB, rpm 3.29 MiB, AppImage 106.50 MiB —
matches `du -h`: 3.3M / 3.3M / 107M.)

Config: `productName openmix`, `version 0.1.0`, `identifier com.openmix.app`,
`bundle.targets all` — deb + rpm + AppImage all produced in one run.
AppImage bundling downloaded `AppRun-x86_64`, `linuxdeploy-x86_64.AppImage`,
and `linuxdeploy-plugin-appimage-x86_64.AppImage` automatically.

## Reproduce

```sh
cd src-tauri
npx -y @tauri-apps/cli@2 build
ls -l target/release/bundle/deb target/release/bundle/rpm target/release/bundle/appimage
```
