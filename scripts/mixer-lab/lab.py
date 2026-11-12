#!/usr/bin/env python3
"""Manual emulator verification; deliberately separate from repository test gates."""
import argparse
import contextlib
import datetime as dt
import hashlib
import functools
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
MATRIX = json.loads((HERE / 'matrix.json').read_text())
CACHE = Path(os.environ.get('OPENFADER_LAB_HOME', Path.home() / '.cache/openfader-mixer-lab')).resolve()
SOURCE = CACHE / 'sidecar' / MATRIX['sidecar_commit']
ADAPTER = CACHE / 'adapter' / MATRIX['sidecar_commit']
TARGET = CACHE / 'target'


def call(args, **kwargs):
    print('+', ' '.join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), check=kwargs.pop('check', True), **kwargs)


def dart():
    candidates = [os.environ.get('DART'), shutil.which('dart'),
                  str(Path.home() / 'flutter-3.47/bin/dart'),
                  str(Path.home() / 'sdks/flutter/bin/dart')]
    for value in candidates:
        if value and Path(value).is_file():
            return value
    raise RuntimeError('Dart unavailable: set DART to the Flutter SDK bin/dart')


def setup():
    SOURCE.parent.mkdir(parents=True, exist_ok=True)
    if not (SOURCE / '.git').exists():
        SOURCE.mkdir(exist_ok=True)
        call(['git', 'init', SOURCE])
        call(['git', '-C', SOURCE, 'remote', 'add', 'origin', MATRIX['sidecar_repository']])
        call(['git', '-C', SOURCE, 'fetch', '--depth=1', 'origin', MATRIX['sidecar_commit']])
        call(['git', '-C', SOURCE, 'checkout', '--detach', 'FETCH_HEAD'])
    verify_source()
    ADAPTER.mkdir(parents=True, exist_ok=True)
    dependencies = '\n'.join(
        f'{name} = {{ path = {json.dumps(str(SOURCE / "rust/crates" / name))} }}'
        for name in ['livedaw-engine', 'livedaw-wing', 'livedaw-x32',
                     'livedaw-session', 'livedaw-ahmidi'])
    (ADAPTER / 'Cargo.toml').write_text(
        '[package]\nname = "openfader-sidecar-lab"\nversion = "0.1.0"\nedition = "2021"\n'
        'publish = false\n[workspace]\n[[bin]]\nname = "openfader-sidecar-lab"\npath = "main.rs"\n'
        '[dependencies]\n' + dependencies + '\n')
    shutil.copyfile(HERE / 'sidecar-host.rs', ADAPTER / 'main.rs')
    # Seed dependency versions from upstream. Cargo adds this small host package.
    if not (ADAPTER / 'Cargo.lock').exists():
        shutil.copyfile(SOURCE / 'rust/Cargo.lock', ADAPTER / 'Cargo.lock')
        call(['cargo', 'build', '--manifest-path', ADAPTER / 'Cargo.toml',
              '--target-dir', TARGET, '-j', '2'])
    else:
        call(['cargo', 'build', '--locked', '--manifest-path', ADAPTER / 'Cargo.toml',
              '--target-dir', TARGET, '-j', '2'])
    call(['cargo', 'build', '--locked', '-p', 'openfader-serve'], cwd=ROOT)
    print('Lab ready. Run: python3 scripts/mixer-lab/lab.py run')


def verify_source():
    sha = subprocess.check_output(['git', '-C', str(SOURCE), 'rev-parse', 'HEAD'], text=True).strip()
    dirty = subprocess.check_output(['git', '-C', str(SOURCE), 'status', '--porcelain'], text=True).strip()
    if sha != MATRIX['sidecar_commit'] or dirty:
        raise RuntimeError('Sidecar source must match the clean pinned commit; never patch the emulator to pass')


@functools.lru_cache(maxsize=1)
def host_binary():
    # Cargo configuration can redirect output even without CARGO_TARGET_DIR.
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'],
        cwd=ROOT, text=True))
    return Path(metadata['target_directory']) / 'debug/openfader-serve'


def fingerprint(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(block)
    return {'path': str(path), 'sha256': digest.hexdigest()}


def preflight():
    verify_source()
    if (ADAPTER / 'main.rs').read_bytes() != (HERE / 'sidecar-host.rs').read_bytes():
        raise RuntimeError('Lab adapter changed; run setup')
    for p in [TARGET / 'debug/openfader-sidecar-lab', host_binary()]:
        if not p.is_file():
            raise RuntimeError(f'Missing {p}; run setup')


@contextlib.contextmanager
def process(args, log):
    with log.open('w') as output:
        proc = subprocess.Popen(list(map(str, args)), cwd=ROOT, stdout=output,
                                stderr=subprocess.STDOUT, start_new_session=True)
        try:
            yield proc
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()


def wait_ready(proc, condition, label, timeout=45):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            raise RuntimeError(f'{label} exited {proc.returncode}; see log')
        if condition():
            return
        time.sleep(.1)
    raise TimeoutError(f'{label} not ready after {timeout}s; see log')


def check_ports(model):
    ports = [('udp', 10023)] if model == 'x32' else (
        [('tcp', 51325)] + ([('tcp', 51328)] if model == 'dlive' else [])
        if model in ('sq5', 'dlive') else [('udp', 2222), ('udp', 2223), ('tcp', 2222)])
    for kind, port in ports:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM if kind == 'udp' else socket.SOCK_STREAM) as s:
            # Match Rust listeners' reuse of closed TCP sockets (TIME_WAIT).
            if kind == 'tcp':
                s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            # Detect wildcard listeners too. Never reuse or kill an unknown process.
            s.bind(('127.0.0.1', port))


def http_ready(port):
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{port}/healthz', timeout=1) as response:
            return response.status == 200
    except OSError:
        return False


@contextlib.contextmanager
def environment(model, directory, with_host=True):
    check_ports(model)
    with contextlib.ExitStack() as stack:
        emulog = directory / 'emulator.log'
        emulator = stack.enter_context(process([TARGET / 'debug/openfader-sidecar-lab', model], emulog))
        wait_ready(emulator, lambda: 'READY ' in emulog.read_text(), 'emulator')
        if not with_host:
            yield emulator, None
            return
        # Ask the OS for a free HTTP port; startup must still succeed after release.
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        hostlog = directory / 'host.log'
        host = stack.enter_context(process([host_binary(), '--bind', '127.0.0.1', '--port', port], hostlog))
        wait_ready(host, lambda: http_ready(port), 'OpenFader host')
        yield emulator, port
        if emulator.poll() is not None or host.poll() is not None:
            raise RuntimeError('Lab process exited during verification')


def revision():
    return {
        'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        'dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT)),
        'diff_sha256': hashlib.sha256(subprocess.check_output(['git', 'diff', 'HEAD'], cwd=ROOT)).hexdigest(),
    }


def run(models, output, quick=False, scenario=None):
    output.mkdir(parents=True, exist_ok=False)
    report = {'time_utc': dt.datetime.now(dt.timezone.utc).isoformat(),
              'sidecar_commit': MATRIX['sidecar_commit'], 'openfader': revision(),
              'scope': 'Emulator protocol evidence only; no UI, hardware, audio, USB or simulator proof',
              'tooling_sha256': {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                                 for p in sorted(HERE.rglob('*')) if p.is_file() and p.suffix in {'.dart', '.rs', '.py', '.json', '.md'}},
              'mode': scenario or ('quick' if quick else 'full'),
              'fixtures': []}
    try:
        with (output / 'build.log').open('w') as build_log:
            call(['cargo', 'build', '--locked', '-p', 'openfader-serve'],
                 cwd=ROOT, stdout=build_log, stderr=subprocess.STDOUT)
        preflight()
        report['executables'] = {
            'openfader': fingerprint(host_binary()),
            'emulator': fingerprint(TARGET / 'debug/openfader-sidecar-lab'),
        }
        report['adapter_lock_sha256'] = hashlib.sha256((ADAPTER / 'Cargo.lock').read_bytes()).hexdigest()
        for model in models:
            fixture = MATRIX['fixtures'][model]
            result = {'model': model, 'driver': fixture['driver'], 'status': 'blocked'}
            report['fixtures'].append(result)
            if fixture['status'] != 'runnable':
                result['reason'] = fixture['reason']
                print(f'BLOCKED {model}: {fixture["reason"]}', flush=True)
                continue
            directory = output / model
            directory.mkdir()
            try:
                with environment(model, directory) as (_, port):
                    stages = []
                    if scenario:
                        script = 'verify-x32-native.dart' if scenario == 'native' else 'verify-x32-full.dart'
                        extra = [] if scenario == 'native' else ['--scenario', scenario]
                        stages.append((scenario, HERE / script, extra, 900))
                    else:
                        stages.append(('basics', HERE / 'verify-basics.dart',
                                       ['--driver', fixture['driver'], '--expected-model', fixture['expected_model']], 180))
                        if model == 'x32':
                            stages.append(('extended', ROOT / 'scripts/verify-x32-emulator.dart',
                                           ['--emulator', '127.0.0.1'], 240))
                            if not quick:
                                stages.extend([('full', HERE / 'verify-x32-full.dart', [], 900),
                                               ('native', HERE / 'verify-x32-native.dart', [], 600)])
                    result['status'] = 'pass'
                    with (directory / 'verification.log').open('w') as log:
                        for name, script, extra, timeout in stages:
                            probe = call([dart(), script, '--ws', f'ws://127.0.0.1:{port}/ws',
                                          '--report', directory / f'{name}.json', *extra],
                                         stdout=log, stderr=subprocess.STDOUT, timeout=timeout, check=False)
                            if probe.returncode == 2:
                                result.update(status='blocked', reason='Explicit coverage gaps; see probe reports')
                            elif probe.returncode != 0:
                                raise subprocess.CalledProcessError(probe.returncode, probe.args)
            except Exception as error:
                result.update(status='fail', reason=str(error))
            result['evidence'] = str(directory)
            print(f'{result["status"].upper()} {model}: {directory}', flush=True)
    except Exception as error:
        report['environment_error'] = str(error)
    finally:
        (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f'Evidence: {output / "report.json"}')
    if 'environment_error' in report or any(r['status'] == 'fail' for r in report['fixtures']):
        return 1
    return 2 if any(r['status'] == 'blocked' for r in report['fixtures']) else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('setup')
    sub.add_parser('matrix')
    p = sub.add_parser('run')
    p.add_argument('models', nargs='*', choices=list(MATRIX['fixtures']) + ['all'])
    p.add_argument('--output', type=Path)
    p.add_argument('--quick', action='store_true', help='Run basic and extended checks without the full X32 feature sweep')
    p.add_argument('--scenario', choices=['routing', 'reconnect', 'native'], help='Focused X32 verification with fresh isolated processes')
    p = sub.add_parser('emulator', help='Foreground emulator for manual app work; Ctrl-C stops it')
    p.add_argument('model', choices=['wing', 'wing-compact', 'wing-rack', 'x32', 'sq5', 'dlive'])
    args = parser.parse_args()
    if args.command == 'setup':
        setup()
    elif args.command == 'matrix':
        print(json.dumps(MATRIX, indent=2))
    elif args.command == 'emulator':
        preflight()
        directory = CACHE / 'sessions' / dt.datetime.now().strftime('%Y%m%d-%H%M%S-%f')
        directory.mkdir(parents=True)
        with environment(args.model, directory, with_host=False) as (emulator, _):
            print(f'Emulator {args.model} ready at 127.0.0.1. Logs: {directory}', flush=True)
            emulator.wait()
    else:
        models = args.models or [m for m, f in MATRIX['fixtures'].items() if f['status'] == 'runnable']
        if 'all' in models:
            models = list(MATRIX['fixtures'])
        output = (args.output or CACHE / 'runs' / dt.datetime.now().strftime('%Y%m%d-%H%M%S-%f')).resolve()
        if args.scenario and (models != ['x32'] or args.quick):
            parser.error('--scenario requires only x32 and cannot combine with --quick')
        return run(list(dict.fromkeys(models)), output, quick=args.quick, scenario=args.scenario)
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        sys.exit(130)
    except Exception as error:
        print(f'ERROR: {error}', file=sys.stderr)
        sys.exit(1)
