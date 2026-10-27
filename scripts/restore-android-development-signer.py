#!/usr/bin/env python3
"""Restore the CI-only development signer from an encrypted Actions secret."""
import base64
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile


def restore_signer(encoded, directory):
    if not encoded:
        raise RuntimeError('Configure the OPENFADER_ANDROID_DEV_KEYSTORE_BASE64 Actions secret before building Android.')
    data = base64.b64decode(encoded.strip(), validate=True)
    if not data or len(data) > 1024 * 1024:
        raise RuntimeError('Invalid Android development keystore size.')
    with tempfile.NamedTemporaryFile(prefix='openfader-development-', suffix='.keystore',
                                     dir=directory, delete=False) as stream:
        path = Path(stream.name)
        os.chmod(path, 0o600)
        stream.write(data)
    try:
        certificate = subprocess.run([
            'keytool', '-exportcert', '-keystore', str(path),
            '-alias', 'androiddebugkey', '-storepass', 'android',
        ], check=True, capture_output=True).stdout
        if not certificate:
            raise RuntimeError('Development signer has no certificate.')
        return path, hashlib.sha256(certificate).hexdigest()
    except Exception:
        path.unlink(missing_ok=True)
        raise


def main():
    path, fingerprint = restore_signer(
        os.environ.get('OPENFADER_ANDROID_DEV_KEYSTORE_BASE64'), os.environ['RUNNER_TEMP'])
    with Path(os.environ['GITHUB_ENV']).open('a') as stream:
        stream.write(f'OPENFADER_ANDROID_DEVELOPMENT_KEYSTORE={path}\n')
        stream.write(f'OPENFADER_ANDROID_EXPECTED_CERT_SHA256={fingerprint}\n')
    print(f'Persistent Android development certificate SHA256: {fingerprint}')


if __name__ == '__main__':
    main()
