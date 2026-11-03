#!/usr/bin/env python3
"""Cross-build Windows native code locally, reusing an unchanged Flutter bundle."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
TARGET = 'x86_64-pc-windows-msvc'
NATIVE = ('openfader-serve.exe', 'openfader-capture-helper.exe',
          'openfader_flutter_bridge.dll')


def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', type=Path, required=True,
                        help='Existing bundle directory with ZIP, checksums and build metadata')
    parser.add_argument('--out', type=Path, default=ROOT / 'artifacts/local-windows')
    args = parser.parse_args()
    base, dest = args.base.resolve(), args.out.resolve()
    if os.name == 'nt':
        parser.error('Use build-windows.ps1 for a complete native Windows build')
    if output('git', 'status', '--porcelain', '--untracked-files=no'):
        parser.error('Commit tracked changes before identifying a build by commit')
    metadata = json.loads((base / 'windows-build.json').read_text())
    base_commit = metadata['commit']
    # Reuse is valid only when all Flutter sources, native runner, assets and
    # dependency locks match. Never relabel an older UI after an app edit.
    subprocess.run(['git', 'diff', '--exit-code', base_commit, '--', 'app'],
                   cwd=ROOT, check=True)
    if output('git', 'ls-files', '--others', '--exclude-standard', '--', 'app'):
        parser.error('Untracked app sources prevent safe Flutter bundle reuse')
    checksums = dict((name.lstrip('*'), sha) for sha, name in
                     (line.split(maxsplit=1) for line in
                      (base / 'windows-SHA256SUMS').read_text().splitlines()))
    for name in ('openfader-windows-x64.zip', 'windows-build.json'):
        if digest(base / name) != checksums[name]:
            parser.error(f'Base artifact checksum mismatch: {name}')
    sdk = Path(os.environ.get('CPAL_ASIO_DIR', ''))
    if not (sdk / 'common/asio.h').is_file():
        parser.error('Set CPAL_ASIO_DIR to the extracted pinned ASIO SDK')
    if not os.environ.get('LIBCLANG_PATH'):
        parser.error('Set LIBCLANG_PATH to the host LLVM library directory')
    env = {**os.environ, 'CARGO_INCREMENTAL': '0', 'XWIN_ARCH': 'x86_64'}
    # asio-sys checks this variable even on a Linux host. xwin supplies the
    # actual compiler, headers and libraries; no vcvarsall.bat is needed.
    env['VCINSTALLDIR'] = env.get('VCINSTALLDIR', 'cargo-xwin')
    command = ['cargo', 'xwin', 'build', '--target', TARGET, '--release', '--locked',
               '-p', 'openfader-serve', '-p', 'openfader-flutter-bridge',
               '-p', 'openfader-audio', '--bin', 'openfader-serve',
               '--bin', 'openfader-capture-helper', '--lib']
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    commit = output('git', 'rev-parse', 'HEAD')
    dest.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='openfader-local-windows-') as tmp:
        staging = Path(tmp)
        with zipfile.ZipFile(base / 'openfader-windows-x64.zip') as archive:
            for entry in archive.infolist():
                path = PurePosixPath(entry.filename)
                if (path.is_absolute() or '..' in path.parts or
                        not path.parts or path.parts[0] != 'openfader' or
                        '\\' in entry.filename or ':' in entry.filename or
                        (entry.external_attr >> 16) & 0o170000 == 0o120000):
                    raise RuntimeError(f'Unsafe bundle entry: {entry.filename}')
            archive.extractall(staging)
        bundle = staging / 'openfader'
        # cargo's configured target directory can be outside this checkout.
        cargo_metadata = json.loads(output('cargo', 'metadata', '--locked', '--no-deps',
                                           '--format-version', '1'))
        native_dir = Path(cargo_metadata['target_directory']) / TARGET / 'release'
        for name in NATIVE:
            shutil.copy2(native_dir / name, bundle / name)
        shutil.copy2(ROOT / 'scripts/check-windows-audio.ps1', bundle / 'check-audio.ps1')
        for name in (*NATIVE, 'openfader.exe', 'flutter_windows.dll',
                     'msvcp140.dll', 'vcruntime140.dll', 'data/app.so',
                     'data/icudtl.dat', 'check-audio.ps1'):
            if not (bundle / name).is_file() or not (bundle / name).stat().st_size:
                raise RuntimeError(f'Missing bundle component: {name}')
        manifest = {
            'platform': 'windows', 'version': (ROOT / 'VERSION').read_text().strip(),
            'commit': commit, 'build_origin': 'local-native-with-reused-flutter-shell',
            'native_build_command': command,
            'rustc': output('rustc', '--version'),
            'cargo_xwin': output('cargo', 'xwin', '--version'),
            'flutter_shell': {
                'commit': base_commit, 'sources_identical_to_commit': commit,
                'base_zip_sha256': digest(base / 'openfader-windows-x64.zip'),
                'flutter': metadata['flutter'],
            },
            'files': {str(p.relative_to(bundle)).replace('\\', '/'): digest(p)
                      for p in sorted(bundle.rglob('*')) if p.is_file()},
            'signing': 'unsigned portable development bundle; no installer',
        }
        if commit != output('git', 'rev-parse', 'HEAD') or output(
                'git', 'status', '--porcelain', '--untracked-files=no'):
            raise RuntimeError('Checkout changed during native build')
        (dest / 'windows-build.json').write_text(json.dumps(manifest, indent=2) + '\n')
        with zipfile.ZipFile(dest / 'openfader-windows-x64.zip', 'w',
                             zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(bundle.rglob('*')):
                if path.is_file():
                    archive.write(path, path.relative_to(staging))
    (dest / 'windows-SHA256SUMS').write_text(''.join(
        f'{digest(dest / name)}  {name}\n' for name in
        ('openfader-windows-x64.zip', 'windows-build.json')))
    print(f'Locally built native components for {commit}: {dest}')


if __name__ == '__main__':
    main()
