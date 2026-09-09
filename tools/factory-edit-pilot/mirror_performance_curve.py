"""Bounded SSH mirror that verifies every performance artifact before publication."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import time

from random_factory_generation import read, write


def mirror(args):
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=args.output) as temporary:
        scratch = Path(temporary)
        def copy(name):
            if Path(name).name != name or name not in ('manifest.json', 'performance.png', 'performance.svg', 'curve.json'):
                raise ValueError('performance-mirror.invalid-artifact-name')
            subprocess.run(['scp', '-q', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10',
                            f'{args.host}:{args.remote}/{name}', str(scratch / name)], check=True, timeout=30)
        copy('manifest.json')
        manifest = read(scratch / 'manifest.json')
        if set(manifest['outputs']) != {'performance.png', 'performance.svg', 'curve.json'}:
            raise ValueError('performance-mirror.manifest-shape')
        for name, expected in manifest['outputs'].items():
            copy(name)
            if hashlib.sha256((scratch / name).read_bytes()).hexdigest() != expected:
                raise ValueError('performance-mirror.snapshot-changed-during-transfer')
        for name in manifest['outputs']:
            (scratch / name).replace(args.output / name)
        (scratch / 'manifest.json').replace(args.output / 'manifest.json')
        write(args.output / 'mirror-status.json', dict(status='verified', received_epoch=time.time(),
            source_updated_epoch=manifest['updated_epoch'], source_status=manifest['status']))
        return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--remote', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--watch', action='store_true')
    parser.add_argument('--seconds', type=float, default=7800)
    args = parser.parse_args()
    if not 0 < args.seconds <= 86400 or any(c.isspace() for c in args.remote) or args.host.startswith('-'):
        parser.error('invalid bounded mirror arguments')
    deadline = time.monotonic() + args.seconds
    while True:
        try:
            result = mirror(args)
            if not args.watch or result['status'] != 'running':
                break
        except Exception as error:
            args.output.mkdir(parents=True, exist_ok=True)
            write(args.output / 'mirror-error.json', dict(code='performance-mirror.transfer-failed', severity='error',
                stage='performance-mirror', entity=args.remote, message=str(error), epoch=time.time()))
            if not args.watch:
                raise
        if time.monotonic() >= deadline:
            raise TimeoutError('performance-mirror.time-budget')
        time.sleep(min(30, deadline - time.monotonic()))
