"""Remove obsolete construction files from completed, accepted fresh inputs."""
import argparse
from collections import Counter
import fcntl
import json
from pathlib import Path
import stat
import time


def prune(root):
    root = Path(root).resolve()
    if not (root / 'manifest.json').is_file():
        raise ValueError(f'construction.cleanup: input manifest missing: {root}')
    counts = Counter()
    began = time.monotonic()
    for directory in root.glob('episode-*'):
        if directory.is_symlink() or not directory.is_dir():
            continue
        suffix = directory.name.removeprefix('episode-')
        if not suffix.isdigit():
            continue
        with (root / f'.slot-{int(suffix):08d}.lock').open('a') as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                counts['busy_inputs'] += 1
                continue
            marker = directory / 'episode.json'
            if not marker.is_file() or marker.is_symlink():
                counts['unpublished_inputs'] += 1
                continue
            record = json.loads(marker.read_text())
            if record.get('schema') != 'blueprint-fresh-episode-v1':
                raise ValueError(f'construction.cleanup: unknown completed input: {marker}')
            protected = {(root / ref['path']).resolve() for ref in record['files'].values()}
            if not all(path.is_relative_to(root) and path.is_file() for path in protected):
                raise ValueError(f'construction.cleanup: missing or foreign actual input: {marker}')
            counts['completed_inputs'] += 1
            for index, attempt in enumerate(record['attempts']):
                if attempt['status'] != 'accepted':
                    counts['failed_attempts_preserved'] += 1
                    continue
                generation = directory / f'attempt-{index}' / 'generation.json'
                if (root / attempt['generation']['path']).resolve() != generation.resolve():
                    raise ValueError(f'construction.cleanup: foreign generation reference: {marker}')
                value = json.loads(generation.read_text())
                admission = directory / f'attempt-{index}' / 'admission'
                if (value.get('status') != 'accepted'
                        or not value['original_attempt'].get('factory_validated')
                        or not admission.resolve().is_relative_to(directory.resolve())):
                    raise ValueError(f'construction.cleanup: invalid accepted attempt: {generation}')
                for name in ('construction.json', 'construction.html'):
                    path = admission / name
                    if path.resolve() in protected:
                        raise ValueError(f'construction.cleanup: file is an actual input: {path}')
                    try:
                        info = path.lstat()
                    except FileNotFoundError:
                        continue
                    if not stat.S_ISREG(info.st_mode):
                        raise ValueError(f'construction.cleanup: expected a regular file: {path}')
                    path.unlink()
                    counts['files_removed'] += 1
                    counts['logical_bytes_removed'] += info.st_size
                    if info.st_nlink == 1:
                        counts['unlinked_allocated_bytes'] += info.st_blocks * 512
    return dict(schema='completed-construction-cleanup-v1', status='complete',
                root=str(root), seconds=time.monotonic() - began, **counts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--summary', type=Path, required=True)
    args = parser.parse_args()
    if args.summary.exists():
        parser.error(f'summary already exists: {args.summary}')
    result = prune(args.inputs)
    args.summary.parent.mkdir(parents=True, exist_ok=True)
    args.summary.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
