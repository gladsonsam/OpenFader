#!/usr/bin/env python3
"""Check a built OpenFader container through its browser and native interfaces."""
import argparse
import base64
import http.client
import hashlib
import json
import shutil
import socket
import ssl
import subprocess
import time
import uuid


def docker(*args):
    return subprocess.check_output(['docker', *args], text=True, timeout=120).strip()


def request(port, path, *, body=None, cookie=None, origin=None):
    headers = {}
    if body is not None:
        headers['Content-Type'] = 'application/json'
        headers['Origin'] = origin or f'http://127.0.0.1:{port}'
    if cookie:
        headers['Cookie'] = cookie
    client = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
    try:
        client.request('POST' if body is not None else 'GET', path,
                       json.dumps(body) if body is not None else None, headers)
        response = client.getresponse()
        return response.status, {key.lower(): value for key, value in response.getheaders()}, response.read()
    finally:
        client.close()


def websocket_status(port, cookie=None):
    key = base64.b64encode(uuid.uuid4().bytes).decode()
    headers = [f'GET /ws HTTP/1.1', f'Host: 127.0.0.1:{port}',
               f'Origin: http://127.0.0.1:{port}', 'Upgrade: websocket',
               'Connection: Upgrade', 'Sec-WebSocket-Version: 13',
               f'Sec-WebSocket-Key: {key}']
    if cookie:
        headers.append(f'Cookie: {cookie}')
    with socket.create_connection(('127.0.0.1', port), timeout=5) as stream:
        stream.sendall(('\r\n'.join(headers) + '\r\n\r\n').encode())
        response = b''
        while b'\r\n' not in response:
            chunk = stream.recv(4096)
            if not chunk or len(response) > 8192:
                raise RuntimeError('Missing WebSocket handshake response')
            response += chunk
        return int(response.split(b'\r\n', 1)[0].split()[1])


def native_request(port, path, *, body=None, token=None):
    # The native protocol pins its generated self-signed certificate. This
    # isolated loopback check compares its DER hash with the server challenge.
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
    context.check_hostname = False
    context.verify_mode = ssl.CERT_NONE
    client = http.client.HTTPSConnection('127.0.0.1', port, context=context, timeout=5)
    try:
        client.connect()
        fingerprint = hashlib.sha256(client.sock.getpeercert(binary_form=True)).hexdigest()
        headers = {'Content-Type': 'application/json'} if body is not None else {}
        if token:
            headers['Authorization'] = f'Bearer {token}'
        client.request('POST' if body is not None else 'GET', path,
                       json.dumps(body) if body is not None else None, headers)
        response = client.getresponse()
        return response.status, json.loads(response.read()), fingerprint
    finally:
        client.close()


def free_port():
    with socket.socket() as stream:
        stream.bind(('127.0.0.1', 0))
        return stream.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image', help='A locally built Docker image')
    args = parser.parse_args()
    if not shutil.which('docker'):
        parser.error('Docker is required to run this check.')
    prefix = f'openfader-check-{uuid.uuid4().hex[:12]}'
    volume = f'{prefix}-data'
    containers = []
    password = 'container-verification-only'

    def start(label, *, password_env=None, persist=False, container_port=8080,
              app_container_port=8845):
        name, host_port, app_host_port = f'{prefix}-{label}', free_port(), free_port()
        while app_host_port == host_port:
            app_host_port = free_port()
        command = ['run', '-d', '--name', name,
                   '-p', f'127.0.0.1:{host_port}:{container_port}',
                   '-p', f'127.0.0.1:{app_host_port}:{app_container_port}',
                   '-e', f'PORT={container_port}', '-e', f'APP_PORT={app_container_port}']
        if password_env is not None:
            command += ['-e', f'OPENFADER_SERVER_PASSWORD={password_env}']
        if persist:
            command += ['-v', f'{volume}:/data']
        command += [args.image, '--driver', 'sim']
        containers.append(name)
        docker(*command)
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            try:
                if request(host_port, '/web/connect')[0] == 200:
                    return name, host_port, app_host_port
            except (OSError, http.client.HTTPException):
                pass
            if docker('inspect', '--format', '{{.State.Running}}', name) != 'true':
                raise RuntimeError(f'{name} exited during startup')
            time.sleep(.5)
        raise TimeoutError('Browser listener did not become ready')

    def verify(port, expected_password):
        status, _, data = request(port, '/web/connect')
        assert status == 200
        assert json.loads(data)['password_required'] == bool(expected_password)
        status, _, index = request(port, '/')
        assert status == 200 and b'flutter_bootstrap.js' in index
        for asset in ('/flutter_bootstrap.js', '/main.dart.js'):
            status, headers, data = request(port, asset)
            assert status == 200 and len(data) > 100
            assert headers['content-type'].startswith('text/javascript')
        assert request(port, '/api/recorder/v1/state')[0] == 401
        assert websocket_status(port) == 401
        assert request(port, '/web/connect', body={},
                       origin='http://another-origin.invalid')[0] == 403
        if expected_password:
            assert request(port, '/web/connect', body={'password': 'wrong'})[0] == 401
            time.sleep(1)
        status, headers, data = request(port, '/web/connect',
                                       body={'password': expected_password or ''})
        assert status == 200, (status, data)
        assert 'client_id' in json.loads(data) and 'token' not in json.loads(data)
        cookie_header = headers['set-cookie']
        assert 'HttpOnly' in cookie_header and 'SameSite=Strict' in cookie_header
        cookie = cookie_header.split(';', 1)[0]
        assert request(port, '/api/recorder/v1/state', cookie=cookie)[0] == 200
        assert websocket_status(port, cookie) == 101
        assert request(port, '/web/disconnect', cookie=cookie, body={})[0] == 200
        assert request(port, '/api/recorder/v1/state', cookie=cookie)[0] == 401

    def verify_native(port, expected_password):
        status, challenge, fingerprint = native_request(port, '/connect')
        assert status == 200 and challenge['fingerprint'] == fingerprint
        assert challenge['password_required'] == bool(expected_password)
        assert native_request(port, '/api/recorder/v1/state')[0] == 401
        if expected_password:
            assert native_request(port, '/connect', body={'password': 'wrong'})[0] == 401
            time.sleep(1)
        status, result, presented = native_request(
            port, '/connect', body={'password': expected_password or ''})
        assert status == 200 and presented == fingerprint
        assert native_request(port, '/api/recorder/v1/state', token=result['token'])[0] == 200
        return fingerprint

    try:
        helper = json.loads(docker('run', '--rm', '--entrypoint',
                                   'openfader-capture-helper', args.image, '--list-devices'))
        assert helper['state'] == 'DEVICES' and isinstance(helper['devices'], list)
        default, port, app_port = start('default')
        verify(port, None)
        verify_native(app_port, None)
        docker('stop', '--time', '5', default)
        protected, port, app_port = start('password', password_env=password, persist=True,
                                          container_port=9090, app_container_port=9190)
        verify(port, password)
        fingerprint = verify_native(app_port, password)
        # Give Docker's built-in healthcheck time to exercise the custom PORT.
        deadline = time.monotonic() + 45
        while docker('inspect', '--format', '{{.State.Health.Status}}', protected) != 'healthy':
            if time.monotonic() >= deadline:
                raise TimeoutError('Custom-port container healthcheck failed')
            time.sleep(1)
        docker('stop', '--time', '5', protected)
        _, port, app_port = start('restarted', persist=True)
        verify(port, password)
        assert verify_native(app_port, password) == fingerprint
        print('Container checks passed: shared Flutter app, browser/native authentication, custom ports and persisted settings/identity.')
    except Exception:
        for name in containers:
            print(f'Container log: {name}')
            subprocess.run(['docker', 'logs', name], check=False)
        raise
    finally:
        for name in containers:
            subprocess.run(['docker', 'rm', '-f', '-v', name], check=False,
                           stdout=subprocess.DEVNULL)
        subprocess.run(['docker', 'volume', 'rm', volume], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


if __name__ == '__main__':
    main()
