"""Durable commits, checkpoint identities and owned paths for the PPO harness."""
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile

SCHEMA = 'ppo-training-harness-native-v4'


class HarnessError(RuntimeError):
    def __init__(self, code, message, stage='harness', entity=None):
        super().__init__(f'ppo-harness.{code}: {message}')
        self.diagnostic = dict(code=f'ppo-harness.{code}', severity='error', stage=stage,
                               entity=str(entity) if entity is not None else None, message=message)


def require(condition, code, message, stage='harness', entity=None):
    if not condition:
        raise HarnessError(code, message, stage, entity)


def diagnostic(error, stage, entity=None):
    return getattr(error, 'diagnostic', dict(code='ppo-harness.execution-failed', severity='error',
        stage=stage, entity=str(entity) if entity is not None else None, message=str(error)))


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    value = hashlib.sha256()
    with open(path, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def sync_file(path):
    with open(path, 'rb') as stream:
        os.fsync(stream.fileno())
    sync_directory(Path(path).parent)


def mkdir(path):
    """Make every new directory entry durable through its existing ancestor."""
    path = Path(path)
    missing, parent = [], path
    while not parent.exists():
        missing.append(parent)
        parent = parent.parent
    for directory in reversed(missing):
        directory.mkdir()
        sync_directory(directory.parent)


def sync_ancestors(path, root):
    parent, root = Path(path).parent, Path(root).resolve()
    require(parent.is_relative_to(root), 'foreign-path', 'cannot sync ancestors outside run', entity=path)
    while parent != root:
        sync_directory(parent)
        parent = parent.parent
    sync_directory(root)


def atomic_file(path, writer):
    path = Path(path)
    descriptor, name = tempfile.mkstemp(prefix=f'.{path.name}-', suffix='.tmp', dir=path.parent)
    try:
        with os.fdopen(descriptor, 'wb') as stream:
            writer(stream)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
        sync_directory(path.parent)
    finally:
        Path(name).unlink(missing_ok=True)


def write(path, value):
    encoded = (json.dumps(value, indent=2, allow_nan=False) + '\n').encode()
    atomic_file(path, lambda stream: stream.write(encoded))


def owned(root, path):
    root = Path(root).resolve()
    path = (root / path).resolve()
    require(path != root and path.is_relative_to(root), 'foreign-path', f'path leaves run: {path}', entity=root)
    return path


def manifest(root, path):
    path = owned(root, path)
    return dict(path=str(path.relative_to(Path(root).resolve())), sha256=sha(path))


def checked_path(root, record):
    path = owned(root, record['path'])
    require(sha(path) == record['sha256'], 'artifact-identity', f'hash mismatch: {path}', entity=path)
    return path


@contextmanager
def writer_lock(root):
    path = Path(root) / '.writer.lock'
    with open(path, 'a+') as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise HarnessError('writer-active', f'another supervisor owns {root}', entity=root) from error
        try:
            yield
        finally:
            fcntl.flock(stream, fcntl.LOCK_UN)


def check_sources(protocol):
    require(protocol.get('schema') == SCHEMA and protocol.get('backend') == 'retained-full-factory-native',
            'runtime-contract', 'prepare a new native-v4 run; replay older runs from their frozen reference checkout', 'prepare/resume')
    for name, expected in protocol['sources'].items():
        require(sha(name) == expected, 'source-drift', f'changed runtime input: {name}', 'prepare/resume', name)


def load_state(root):
    root = Path(root)
    state = read(root / 'state.json')
    require(state['schema'] == SCHEMA and state['protocol_sha256'] == sha(root / 'protocol.json'),
            'state-identity', 'state schema or frozen protocol mismatch', entity=root)
    return state


def save_checkpoint(root, path, model_set, protocol_sha256, job, batch):
    import torch
    from full_factory_on_policy import parameter_hash
    path = owned(root, path)
    actor, critic, aopt, copt, rng = model_set
    saved = dict(schema=SCHEMA, protocol_sha256=protocol_sha256, job=job, batch=batch,
                 policy=actor.state_dict(), critic=critic.state_dict(),
                 policy_sha256=parameter_hash(actor), critic_sha256=parameter_hash(critic),
                 actor_optimizer=aopt.state_dict(), critic_optimizer=copt.state_dict(),
                 shuffle_rng=rng.get_state(), rng=torch.get_rng_state(),
                 cuda_rng=torch.cuda.get_rng_state_all() if next(actor.parameters()).is_cuda else None)
    atomic_file(path, lambda stream: torch.save(saved, stream))
    return dict(**manifest(root, path), policy_sha256=saved['policy_sha256'], critic_sha256=saved['critic_sha256'])


def restore_checkpoint(root, record, model_set, protocol_sha256, job, batch):
    import torch
    from full_factory_on_policy import parameter_hash
    path = checked_path(root, record)
    saved = torch.load(path, map_location='cpu', weights_only=False)
    require((saved['schema'], saved['protocol_sha256'], saved['job'], saved['batch']) ==
            (SCHEMA, protocol_sha256, job, batch), 'checkpoint-identity', f'checkpoint contract: {path}', entity=path)
    actor, critic, aopt, copt, rng = model_set
    for value, key in zip(model_set[:4], ('policy', 'critic', 'actor_optimizer', 'critic_optimizer')):
        value.load_state_dict(saved[key])
    require(parameter_hash(actor) == record['policy_sha256'] == saved['policy_sha256'] and
            parameter_hash(critic) == record['critic_sha256'] == saved['critic_sha256'],
            'checkpoint-weights', f'checkpoint weights: {path}', entity=path)
    rng.set_state(saved['shuffle_rng'])
    torch.set_rng_state(saved['rng'])
    if saved['cuda_rng'] is not None:
        require(next(actor.parameters()).is_cuda, 'checkpoint-device', 'CUDA checkpoint requires CUDA learning')
        torch.cuda.set_rng_state_all(saved['cuda_rng'])
    return saved


def commit_phase(root, state, job_index, checkpoint, record):
    """The state pointer is published last; unreferenced artifacts never advance it."""
    job = state['jobs'][job_index]
    job['checkpoint'] = checkpoint
    job['records'].append(record)
    job['next_phase'] += 1
    write(Path(root) / 'state.json', state)
