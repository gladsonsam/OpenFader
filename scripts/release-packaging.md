# Windows and Android release packaging

Before merging, run the **Build and release apps** workflow with `workflow_dispatch`, select the working branch and `windows-android`, and inspect both artifacts. The Android job requires the existing `OPENFADER_ANDROID_DEV_KEYSTORE_BASE64` Actions secret. It restores and verifies the persistent development certificate; the APK is not Play Store signed.

A push of a `v` tag matching `VERSION` builds both platforms. Only after both jobs succeed, the publish job checks that the remote tag still resolves to the triggering commit and creates the GitHub release. Do not move release tags. The Windows setup installs per user and is unsigned. Platform-specific SHA256SUMS and build metadata accompany the installer, portable ZIP, universal APK and Android app bundle.
