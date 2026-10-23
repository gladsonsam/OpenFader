#!/usr/bin/env python3
"""Build Flutter bundles with the real Rust engine, on their host OS."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / 'apps/app'
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('platform', choices=['ios', 'macos', 'linux', 'windows', 'android'])
    platform = parser.parse_args().platform
    expected = {'ios': 'darwin', 'macos': 'darwin', 'linux': 'linux',
                'windows': 'win32', 'android': 'linux'}[platform]
    if sys.platform != expected:
        parser.error(f'{platform} builds require a {expected} host')
    OUT.mkdir(exist_ok=True)
    version = (ROOT / 'VERSION').read_text().strip()
    number = os.environ.get('OPENMIX_BUILD_NUMBER', '1')
    flutter = shutil.which('flutter') or os.environ.get('FLUTTER_BIN')
    if not flutter:
        parser.error('flutter not on PATH (set FLUTTER_BIN or add flutter to PATH)')
    # Windows .bat launchers need cmd.exe; subprocess handles them on Windows.
    run(flutter, 'pub', 'get', '--enforce-lockfile')
    flags = [f'--build-name={version}', f'--build-number={number}', '--no-pub']
    outputs = []

    def output(name):
        path = OUT / name
        outputs.append(path)
        return path

    if platform == 'ios':
        run(flutter, 'build', 'ipa', '--release', '--no-codesign', *flags)
        archive = APP / 'build/ios/archive/Runner.xcarchive'
        zip_tree(archive, output('openmix-ios-unsigned.xcarchive.zip'))
        # An unsigned IPA can be re-signed later; it cannot be installed as-is.
        staging = APP / 'build/ci-ios'
        shutil.rmtree(staging, ignore_errors=True)
        payload = staging / 'Payload'
        payload.mkdir(parents=True)
        shutil.copytree(archive / 'Products/Applications/Runner.app', payload / 'Runner.app', symlinks=True)
        run('ditto', '-c', '-k', '--keepParent', payload, output('openmix-ios-unsigned.ipa'))
        run(flutter, 'build', 'ios', '--simulator', '--debug', *flags)
        zip_tree(APP / 'build/ios/iphonesimulator/Runner.app', output('openmix-ios-simulator.app.zip'))
    elif platform == 'macos':
        run(flutter, 'build', 'macos', '--release', *flags)
        app = APP / 'build/macos/Build/Products/Release/OpenMix.app'
        # Ad-hoc sealing only. No distribution certificate or notarization.
        run('codesign', '--force', '--sign', '-', '--preserve-metadata=identifier,entitlements,flags,runtime', app)
        run('codesign', '--verify', '--deep', '--strict', app)
        zip_tree(app, output('openmix-macos-universal.app.zip'))
        dmg = output('openmix-macos-universal.dmg')
        dmg.unlink(missing_ok=True)
        run('hdiutil', 'create', '-volname', 'OpenMix', '-srcfolder', app, '-ov', '-format', 'UDZO', dmg)
    elif platform in ('linux', 'windows'):
        run('cargo', 'build', '--locked', '--release', '-p', 'openmix-flutter-bridge', cwd=ROOT)
        run(flutter, 'build', platform, '--release', *flags)
        if platform == 'linux':
            bundle = APP / 'build/linux/x64/release/bundle'
            shutil.copy2(ROOT / 'target/release/libopenmix_flutter_bridge.so', bundle / 'lib')
            with tarfile.open(output('openmix-linux-x64.tar.gz'), 'w:gz') as archive:
                archive.add(bundle, arcname='openmix')
        else:
            bundle = APP / 'build/windows/x64/runner/Release'
            shutil.copy2(ROOT / 'target/release/openmix_flutter_bridge.dll', bundle)
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
            zip_tree(bundle, output('openmix-windows-x64.zip'), prefix='openmix')
    else:
        # Overwrite the checked-in bridge libraries: never ship stale engine code.
        run('cargo', 'ndk', '-t', 'arm64-v8a', '-t', 'armeabi-v7a', '-t', 'x86_64',
            '-o', APP / 'android/app/src/main/jniLibs', 'build', '--locked',
            '--release', '-p', 'openmix-flutter-bridge', cwd=ROOT)
        run(flutter, 'build', 'apk', '--release', *flags)
        shutil.copy2(APP / 'build/app/outputs/flutter-apk/app-release.apk', output('openmix-android-dev-signed.apk'))
        run(flutter, 'build', 'appbundle', '--release', *flags)
        shutil.copy2(APP / 'build/app/outputs/bundle/release/app-release.aab', output('openmix-android-dev-signed.aab'))

    metadata = {
        'platform': platform, 'version': version, 'build_number': number,
        'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'flutter': json.loads(subprocess.check_output([flutter, '--version', '--machine'], cwd=APP, text=True)),
        'signing': 'development only; no distribution signing or store upload',
    }
    output(f'{platform}-build.json').write_text(json.dumps(metadata, indent=2) + '\n')
    checksums = []
    for path in outputs:
        if not path.is_file() or path.stat().st_size == 0:
            raise RuntimeError(f'Missing or empty artifact: {path}')
        with path.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        checksums.append(f'{digest}  {path.name}\n')
    (OUT / f'{platform}-SHA256SUMS').write_text(''.join(checksums))


if __name__ == '__main__':
    main()
