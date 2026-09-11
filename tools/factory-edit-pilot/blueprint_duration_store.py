"""Lossless duration-run storage and immutable admitted-input caching."""
import gzip
import shutil
from contextlib import contextmanager
import fcntl
import os

import torch

from blueprint_ppo_store import reference
from blueprint_ppo_update import require


@contextmanager
def exclusive_run(output):
    with (output / '.run.lock').open('a+') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError(f'blueprint.duration: another trainer owns {output}') from None
        lock.seek(0)
        lock.truncate()
        lock.write(str(os.getpid()) + '\n')
        lock.flush()
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def save_compact(path, value, level):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    with gzip.open(temporary, 'wb', compresslevel=level) as stream:
        torch.save(value, stream)
    temporary.replace(path)


class SequenceCache:
    def __init__(self):
        self.entries = {}

    def resolve(self, buffer, evidence, ordinal):
        require(set(self.entries) <= set(buffer.state['entries']), 'blueprint.duration-cache: no eviction')
        result = []
        for identity in sorted(buffer.state['entries']):
            entry = buffer.state['entries'][identity]
            require(entry['admitted_update'] < ordinal, 'blueprint.duration-cache: prior completed episode')
            if identity not in self.entries:
                rows = []
                for label in entry['labels']:
                    require(label['reference'][0] < ordinal, 'blueprint.duration-cache: prior source chunk')
                    sample = evidence.sample(label['reference'])
                    rows.append(dict(input=sample['input'], index=sample['index'], stage=label['stage'],
                                     reference=label['reference'], return_to_terminal=label['return_to_terminal']))
                self.entries[identity] = dict(id=identity, admitted_update=entry['admitted_update'], rows=rows)
            result.append(self.entries[identity])
        return result


def boundary_stop(seconds, duration, streams, ordinal, maximum, requested=False):
    if any(stream.session is not None for stream in streams):
        return None
    if requested:
        return 'requested_stop'
    if seconds >= duration:
        return 'duration'
    if ordinal >= maximum:
        return 'update_limit'
    return None


def retained_updates(job):
    keep = {0, 1, 2, 3, *(e['update'] for e in job['evaluations'])}
    keep.update(range(max(0, job['update'] - 3), job['update'] + 1))
    if 'first_admission_update' in job:
        keep.update((job['first_admission_update'], job['first_admission_update'] + 1))
    return keep


def prune_checkpoints(output, directory, job):
    keep = retained_updates(job)
    for path in (directory / 'checkpoints').glob('*.pt.gz'):
        if int(path.name.split('.')[0]) not in keep:
            job['removed_checkpoints'].append(reference(path, output))
            path.unlink()


def storage_guard(output, config):
    used = sum(p.stat().st_size for p in output.rglob('*') if p.is_file())
    free = shutil.disk_usage(output).free
    require(used < config['limits']['max_output_bytes'], f'blueprint.duration-storage: output byte limit at {output}')
    require(free >= config['duration']['min_free_bytes'], f'blueprint.duration-storage: free space reserve at {output}')
    return dict(output_bytes=used, free_bytes=free)
