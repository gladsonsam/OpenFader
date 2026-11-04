# Windows and Android release packaging

Use local builds for development and hardware verification. Do not dispatch Actions builds or checks without the owner's explicit request; they consume Actions usage. The existing **Build and release apps** workflow remains available for owner-requested releases. Its Android job requires the existing `OPENFADER_ANDROID_DEV_KEYSTORE_BASE64` Actions secret. It restores and verifies the persistent development certificate; the APK is not Play Store signed.

## Local Windows audio builds from Linux

For Rust-only changes, `build-windows-local.py` cross-builds the MSVC host,
capture helper and bridge on the development machine. It reuses an existing
portable Flutter shell only after verifying that every tracked file under
`app/` is unchanged from that shell's recorded commit, and verifies the base
ZIP and metadata against their checksums. Untracked app files or tracked
uncommitted changes prevent packaging. The result records both native and
shell provenance and hashes every bundled file. It does not start Actions.

Install LLVM/clang/lld/libclang and `cargo-xwin` 0.23.1 on the development
machine, add the `x86_64-pc-windows-msvc` Rust target, and extract the pinned
ASIO SDK from `provision-windows-ci.ps1` (verify its SHA256). For example:

```sh
export PATH="/usr/lib/llvm-19/bin:$PATH"
export LIBCLANG_PATH=/usr/lib/llvm-19/lib
export CPAL_ASIO_DIR="$HOME/.cache/openfader-windows/asio/ASIOSDK"
export XWIN_CACHE_DIR="$HOME/.cache/openfader-windows/xwin"
python3 scripts/build-windows-local.py --base /path/to/existing-bundle
```

The base directory must contain `openfader-windows-x64.zip`,
`windows-build.json` and `windows-SHA256SUMS`. Reusing an already built
artifact needs no new Actions job. A Flutter source or dependency change
requires a full Windows build using `build-windows.ps1` on a development
Windows installation or disposable VM; Flutter does not support a full
Windows desktop cross-build from Linux.

For a full Windows build in a local development VM, run
`python scripts/ci-build-app.py windows --portable-only` from an x64 Visual
C++ developer shell with Flutter, Rust, LLVM and the ASIO SDK available.
This produces the portable ZIP without installing or invoking Inno Setup.
Build metadata records a tracked working-tree patch hash when testing
uncommitted changes and rejects sources that change during the build.

Deploy only the extracted portable folder and test data to the recording PC.
Keep bundled runtime DLLs alongside the executables; install no Rust,
Flutter, C++ tools, LLVM or ASIO SDK there. The manufacturer ASIO driver is
the existing prerequisite. The app also persists recorder data under
`%APPDATA%\OpenFader\Recorder`, host files under
`%LOCALAPPDATA%\OpenFader\Recorder`, and Flutter preferences under
`%APPDATA%\com.openfader\OpenFader`. Inventory existing files before testing
and record newly created files for removal; preserve pre-existing recordings
and preferences. Exported audio stays at the explicitly selected destination.

A push of a `v` tag matching `VERSION` creates a draft release and builds both platforms. Each build uploads directly to that draft, avoiding the Actions artifact storage quota. Only after both jobs succeed, the publish job checks the tag target, asset commit metadata and checksums before publishing the release. Failed builds leave an unpublished draft for inspection. Do not move release tags. The Windows setup installs per user and is unsigned. Platform-specific SHA256SUMS and build metadata accompany the installer, portable ZIP, universal APK and Android app bundle.


## Owner-authorized release refresh

Only refresh an existing version when the owner explicitly requests replacement. Keep the current release public until PR checks and Windows/Android smoke builds pass. Preserve its original commit on an archive branch before changing the tag. Then make the release a draft, move the version tag to the tested merge commit using an explicit old-object lease, and let the tagged build replace the draft assets. The publish job must still verify both platform checksums and exact commit metadata before publishing. Preserve the Android signing key and use a newer build number so installed APKs can update.

A failed refresh remains a draft; never publish a mix of old and new assets. Do not reuse or overwrite a version without the owner's explicit request.
