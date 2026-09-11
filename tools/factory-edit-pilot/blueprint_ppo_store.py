"""Bounded experiment I/O and source-bound learner checkpoints."""
import gzip
import hashlib
import json
from pathlib import Path

import torch

from blueprint_ppo_update import digest, learners, require

ROOT = Path(__file__).resolve().parents[2]


def read(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError) as error:
        raise ValueError(f'blueprint.input: {path}: {error}') from None


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.tmp')
    with gzip.open(temporary, 'wb', compresslevel=1) as stream:
        torch.save(value, stream)
    temporary.replace(path)


def load(path):
    with gzip.open(path, 'rb') as stream:
        return torch.load(stream, map_location='cpu', weights_only=False)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def reference(path, root):
    return dict(path=str(Path(path).relative_to(root)), sha256=sha(path))


def checked(reference, root):
    path = (root / reference['path']).resolve()
    require(path.is_relative_to(root.resolve()) and sha(path) == reference['sha256'], f'checkpoint/evidence identity: {path}')
    return path


def model_checkpoint(model, actor, critic, sampler, shuffle, seed, update, streams, protocol_hash):
    return dict(schema='blueprint-ppo-checkpoint-v1', protocol_sha256=protocol_hash, seed=seed, update=update,
                model=model.state_dict(), actor_optimizer=actor.state_dict(), critic_optimizer=critic.state_dict(),
                sampler_rng=sampler.get_state(), shuffle_rng=shuffle.get_state(), streams=[s.snapshot() for s in streams],
                model_sha256=digest(model.state_dict()))


def restore_learners(saved, config, protocol_hash):
    require(saved['schema'] == 'blueprint-ppo-checkpoint-v1' and saved['protocol_sha256'] == protocol_hash, 'new learner checkpoint/protocol identity')
    model, actor, critic = learners(saved['seed'], config)
    model.load_state_dict(saved['model'], strict=True)
    actor.load_state_dict(saved['actor_optimizer'])
    critic.load_state_dict(saved['critic_optimizer'])
    require(digest(model.state_dict()) == saved['model_sha256'], 'model checkpoint digest')
    require(all(g['lr'] == config['learning_rate'] for optimizer in (actor, critic) for g in optimizer.param_groups), 'checkpoint learning rate')
    sampler, shuffle = torch.Generator(), torch.Generator()
    sampler.set_state(saved['sampler_rng'])
    shuffle.set_state(saved['shuffle_rng'])
    return model, actor, critic, sampler, shuffle
