# Windows and Android release packaging

Before merging, run the **Build and release apps** workflow with `workflow_dispatch`, select the working branch and `windows-android`, and inspect both build results. Disable `upload-artifacts` for a smoke run when the Actions artifact quota is full. The Android job requires the existing `OPENFADER_ANDROID_DEV_KEYSTORE_BASE64` Actions secret. It restores and verifies the persistent development certificate; the APK is not Play Store signed.

A push of a `v` tag matching `VERSION` creates a draft release and builds both platforms. Each build uploads directly to that draft, avoiding the Actions artifact storage quota. Only after both jobs succeed, the publish job checks the tag target, asset commit metadata and checksums before publishing the release. Failed builds leave an unpublished draft for inspection. Do not move release tags. The Windows setup installs per user and is unsigned. Platform-specific SHA256SUMS and build metadata accompany the installer, portable ZIP, universal APK and Android app bundle.


## Owner-authorized release refresh

Only refresh an existing version when the owner explicitly requests replacement. Keep the current release public until PR checks and Windows/Android smoke builds pass. Preserve its original commit on an archive branch before changing the tag. Then make the release a draft, move the version tag to the tested merge commit using an explicit old-object lease, and let the tagged build replace the draft assets. The publish job must still verify both platform checksums and exact commit metadata before publishing. Preserve the Android signing key and use a newer build number so installed APKs can update.

A failed refresh remains a draft; never publish a mix of old and new assets. Do not reuse or overwrite a version without the owner's explicit request.
