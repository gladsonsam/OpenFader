#!/usr/bin/env python3
"""Build Flutter bundles with the real Rust engine, on their host OS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import ssl
import subprocess
import sys
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / 'app'
OUT = ROOT / 'artifacts'


def run(*args, cwd=APP):
    subprocess.run([str(arg) for arg in args], cwd=cwd, check=True)


def zip_tree(source, destination, prefix=None):
    # Apple bundles contain framework symlinks. ditto preserves them and modes.
    if sys.platform == 'darwin':
        run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', source, destination)
    else:
        with zipfile.ZipFile(destination, 'w', zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(source.rglob('*')):
                if path.is_file():
                    archive.write(path, Path(prefix or source.name) / path.relative_to(source))


def android_signer(apk):
    sdk = os.environ.get('ANDROID_HOME') or os.environ.get('ANDROID_SDK_ROOT')
    candidates = sorted((Path(sdk) / 'build-tools').glob('*/apksigner')) if sdk else []
    executable = shutil.which('apksigner') or (str(candidates[-1]) if candidates else None)
    if not executable:
        raise RuntimeError('apksigner is required to verify the Android artifact certificate.')
    report = subprocess.check_output([executable, 'verify', '--print-certs-pem', str(apk)], text=True)
    certificates = re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----', report, re.S)
    fingerprints = {
        hashlib.sha256(ssl.PEM_cert_to_DER_cert(certificate)).hexdigest()
        for certificate in certificates
    }
    if len(fingerprints) != 1:
        raise RuntimeError('Expected exactly one verified Android development signer.')
    fingerprint = fingerprints.pop()
    expected = os.environ.get('OPENFADER_ANDROID_EXPECTED_CERT_SHA256')
    if os.environ.get('GITHUB_ACTIONS') == 'true' and not expected:
        raise RuntimeError('Android CI has no expected development certificate fingerprint.')
    if expected and fingerprint != expected.lower():
        raise RuntimeError('APK certificate does not match the restored development signer.')
    return {'certificate_sha256': fingerprint, 'persistent_ci': bool(expected)}


def verify_android_bundle(bundle, fingerprint):
    report = subprocess.check_output(['jarsigner', '-J-Duser.language=en', '-verify', str(bundle)], text=True)
    if 'jar verified.' not in report:
        raise RuntimeError('Android app bundle signature could not be verified.')
    certificate = subprocess.check_output([
        'keytool', '-printcert', '-jarfile', str(bundle), '-rfc'], text=True)
    certificates = re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----', certificate, re.S)
    if len(certificates) != 1:
        raise RuntimeError('Expected exactly one Android app bundle development certificate.')
    digest = hashlib.sha256(ssl.PEM_cert_to_DER_cert(certificates[0])).hexdigest()
    if digest != fingerprint:
        raise RuntimeError('Android app bundle and APK signing certificates differ.')
    return digest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=['ios', 'macos', 'linux', 'windows', 'android'])
    parser.add_argument('--portable-only', action='store_true',
                        help='Package Windows ZIP without requiring Inno Setup')
    args = parser.parse_args()
    platform = args.platform
    if args.portable_only and platform != 'windows':
        parser.error('--portable-only is available for Windows')
    expected = {'ios': 'darwin', 'macos': 'darwin', 'linux': 'linux',
                'windows': 'win32', 'android': 'linux'}[platform]
    if sys.platform != expected and not (platform == 'android' and sys.platform == 'darwin'):
        parser.error(f'{platform} builds require a {expected} host')
    OUT.mkdir(exist_ok=True)
    if subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=ROOT).strip():
        raise RuntimeError('Untracked sources prevent build identity. Commit them or mark intentional new sources with git add -N.')
    source_commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    source_patch = subprocess.check_output(['git', 'diff', '--binary', '--full-index', 'HEAD'], cwd=ROOT)
    version = (ROOT / 'VERSION').read_text().strip()
    number = os.environ.get('OPENFADER_BUILD_NUMBER', '1')
    flutter = shutil.which('flutter') or os.environ.get('FLUTTER_BIN')
    if not flutter:
        parser.error('flutter not on PATH (set FLUTTER_BIN or add flutter to PATH)')
    # Windows .bat launchers need cmd.exe; subprocess handles them on Windows.
    run(flutter, 'pub', 'get', '--enforce-lockfile')
    flags = [f'--build-name={version}', f'--build-number={number}', '--no-pub']
    outputs = []
    signing = None

    def output(name):
        path = OUT / name
        outputs.append(path)
        return path

    if platform == 'ios':
        run(flutter, 'build', 'ipa', '--release', '--no-codesign', *flags)
        archive = APP / 'build/ios/archive/Runner.xcarchive'
        zip_tree(archive, output('openfader-ios-unsigned.xcarchive.zip'))
        # An unsigned IPA can be re-signed later; it cannot be installed as-is.
        staging = APP / 'build/ci-ios'
        shutil.rmtree(staging, ignore_errors=True)
        payload = staging / 'Payload'
        payload.mkdir(parents=True)
        shutil.copytree(archive / 'Products/Applications/Runner.app', payload / 'Runner.app', symlinks=True)
        run('ditto', '-c', '-k', '--keepParent', payload, output('openfader-ios-unsigned.ipa'))
        run(flutter, 'build', 'ios', '--simulator', '--debug', *flags)
        zip_tree(APP / 'build/ios/iphonesimulator/Runner.app', output('openfader-ios-simulator.app.zip'))
    elif platform == 'macos':
        run(flutter, 'build', 'macos', '--release', *flags)
        app = APP / 'build/macos/Build/Products/Release/OpenFader.app'
        # Ad-hoc sealing only. No distribution certificate or notarization.
        run('codesign', '--force', '--sign', '-', '--preserve-metadata=identifier,entitlements,flags,runtime', app)
        run('codesign', '--verify', '--deep', '--strict', app)
        zip_tree(app, output('openfader-macos-universal.app.zip'))
        dmg = output('openfader-macos-universal.dmg')
        dmg.unlink(missing_ok=True)
        run('hdiutil', 'create', '-volname', 'OpenFader', '-srcfolder', app, '-ov', '-format', 'UDZO', dmg)
    elif platform in ('linux', 'windows'):
        if platform == 'linux':
            run('cargo', 'build', '--locked', '--release', '--target-dir', ROOT / 'target', '-p', 'openfader-flutter-bridge',
                '-p', 'openfader-serve', '-p', 'openfader-audio', cwd=ROOT)
        # Windows CMake builds the host, helper and bridge as part of this
        # invocation. A separate Cargo invocation churns the shared cache.
        run(flutter, 'build', platform, '--release', *flags)
        if platform == 'linux':
            bundle = APP / 'build/linux/x64/release/bundle'
            shutil.copy2(ROOT / 'target/release/libopenfader_flutter_bridge.so', bundle / 'lib')
            with tarfile.open(output('openfader-linux-x64.tar.gz'), 'w:gz') as archive:
                archive.add(bundle, arcname='openfader')
        else:
            bundle = APP / 'build/windows/x64/runner/Release'
            shutil.copy2(ROOT / 'target/release/openfader_flutter_bridge.dll', bundle)
            shutil.copy2(ROOT / 'scripts/check-windows-audio.ps1', bundle / 'check-audio.ps1')
            # Portable builds need the MSVC runtime on clean Windows machines.
            vswhere = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
            installation = subprocess.check_output([
                str(vswhere), '-latest', '-products', '*', '-requires',
                'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
                '-property', 'installationPath'], text=True).strip()
            redist = sorted((Path(installation) / 'VC/Redist/MSVC').glob('*/x64/Microsoft.VC*.CRT'))
            if not installation or not redist:
                raise RuntimeError('Visual C++ redistributable DLLs not found')
            for library in redist[-1].glob('*.dll'):
                shutil.copy2(library, bundle)
            for name in ('openfader.exe', 'openfader-serve.exe', 'openfader-capture-helper.exe',
                         'openfader_flutter_bridge.dll', 'flutter_windows.dll', 'msvcp140.dll', 'vcruntime140.dll'):
                if not (bundle / name).is_file():
                    raise RuntimeError(f'Missing Windows bundle component: {name}')
            if not args.portable_only:
                iscc = Path(os.environ['ProgramFiles(x86)']) / 'Inno Setup 6/ISCC.exe'
                if not iscc.is_file():
                    raise RuntimeError('Inno Setup 6 compiler not found on runner')
                run(iscc, f'/DAppVersion={version}', ROOT / 'scripts/windows-installer.iss', cwd=ROOT)
                output(f'OpenFader-{version}-windows-x64-setup.exe')
            zip_tree(bundle, output('openfader-windows-x64.zip'), prefix='openfader')
    else:
        # Gradle builds the Rust bridge for every packaged ABI, including local
        # Flutter builds. One path prevents missing or stale native engines.
        run(flutter, 'build', 'apk', '--release', *flags)
        apk = APP / 'build/app/outputs/flutter-apk/app-release.apk'
        signing = android_signer(apk)
        with zipfile.ZipFile(apk) as archive:
            for abi in ('armeabi-v7a', 'arm64-v8a', 'x86_64'):
                name = f'lib/{abi}/libopenfader_flutter_bridge.so'
                if name not in archive.namelist() or archive.getinfo(name).file_size == 0:
                    raise RuntimeError(f'APK missing native bridge: {name}')
        shutil.copy2(apk, output(f'OpenFader-{version}-android.apk'))
        run(flutter, 'build', 'appbundle', '--release', *flags)
        bundle = APP / 'build/app/outputs/bundle/release/app-release.aab'
        signing['bundle_certificate_sha256'] = verify_android_bundle(bundle, signing['certificate_sha256'])
        shutil.copy2(bundle, output('openfader-android-dev-signed.aab'))

    if (source_commit != subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
            or source_patch != subprocess.check_output(['git', 'diff', '--binary', '--full-index', 'HEAD'], cwd=ROOT)):
        raise RuntimeError('Tracked sources changed during the build; rebuild before packaging')
    metadata = {
        'platform': platform, 'version': version, 'build_number': number,
        'commit': source_commit,
        'working_tree_dirty': bool(source_patch),
        'patch_sha256': hashlib.sha256(source_patch).hexdigest() if source_patch else None,
        'flutter': json.loads(subprocess.check_output([flutter, '--version', '--machine'], cwd=APP, text=True)),
        'signing': 'development only; no distribution signing or store upload',
    }
    if signing is not None:
        metadata['android_signing'] = signing
    if platform == 'windows':
        metadata['build_origin'] = ('github-actions' if os.environ.get('GITHUB_ACTIONS') == 'true'
                                    else 'local-windows-native')
        metadata['files'] = {}
        for path in sorted(bundle.rglob('*')):
            if path.is_file():
                with path.open('rb') as stream:
                    metadata['files'][path.relative_to(bundle).as_posix()] = hashlib.file_digest(stream, 'sha256').hexdigest()
    output(f'{platform}-build.json').write_text(json.dumps(metadata, indent=2) + '\n')
    checksums = []
    for path in outputs:
        if not path.is_file() or path.stat().st_size == 0:
            raise RuntimeError(f'Missing or empty artifact: {path}')
        with path.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        checksums.append(f'{digest}  {path.name}\n')
    (OUT / f'{platform}-SHA256SUMS').write_bytes(''.join(checksums).encode('utf-8'))


if __name__ == '__main__':
    main()
