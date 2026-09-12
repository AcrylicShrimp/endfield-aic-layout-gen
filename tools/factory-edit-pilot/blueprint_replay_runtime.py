"""Explicit child learner and durable mixed-replay update boundary."""
from copy import deepcopy
from pathlib import Path
import time

import torch

from blueprint_execution_update import update_packed
from blueprint_ppo_store import load, reference, save, write
from blueprint_ppo_update import digest, learners
from blueprint_replay_buffer import ReplayBuffer
from blueprint_replay_contract import Sources, check
from blueprint_success_buffer import Evidence
from blueprint_duration_metrics import training_quality

CHECKPOINT = 'blueprint-replay-balance-checkpoint-v1'


def admitted_sources(directory, seed):
    from blueprint_ppo_store import read
    directory = Path(directory)
    sources = Sources()
    manifest = read(sources.path(directory / 'manifest.json'))
    parent_ref = next(p['file'] for p in manifest['parents'] if p['seed'] == seed)
    parent = load(sources.reference(directory, parent_ref))
    imports = load(sources.reference(directory, manifest['imports']))['sequences']
    return parent, imports, dict(manifest_sha256=sources.bindings[str((directory / 'manifest.json').resolve())],
        parent=parent_ref, imports=manifest['imports'], seed=seed,
        parent_protocol_sha256=parent['parent_protocol_sha256'], parent_checkpoint_sha256=parent['parent_checkpoint_sha256'])


def child_config(parent, spec, directory, readiness=False):
    config = deepcopy(parent['parent_protocol']['config'])
    config['schema'] = spec['schema']
    config['ppo'] = deepcopy(spec['ppo'])
    config['training'] = dict(spec['training'], updates=spec['child_updates'])
    config['training_input'].update(directory=str(Path(directory).resolve()),
        seed_base=spec['readiness']['seed_base'] if readiness else spec['training_inputs']['seed_base'],
        namespace=deepcopy(spec['training_inputs']['namespace']))
    config['evaluation'].update(spec['evaluation'])
    return config


class Child:
    def __init__(self, parent, imports, binding, spec, arm, device, protocol_hash, saved=None):
        self.seed, self.spec, self.arm = parent['seed'], spec, arm
        self.binding, self.protocol_hash = binding, protocol_hash
        if saved is not None:
            check(saved['schema'] == CHECKPOINT and saved['protocol_sha256'] == protocol_hash
                and saved['parent'] == binding and saved['arm'] == arm and saved['device'] == device,
                'parent-incompatible', 'restore', 'child checkpoint identity differs')
            check(saved['global_update'] == spec['parent_update'] + saved['child_update'],
                'parent-incompatible', 'restore', 'child and global clocks disagree')
        source = saved if saved is not None else parent['learner']
        self.model, self.actor, self.critic = learners(self.seed, spec['ppo'])
        self.model.load_state_dict(source['model'], strict=True)
        self.model.to(device)
        self.actor.load_state_dict(deepcopy(source['actor_optimizer']))
        self.critic.load_state_dict(deepcopy(source['critic_optimizer']))
        check(bool(self.actor.state) and bool(self.critic.state), 'parent-incompatible', 'restore', 'empty parent Adam state')
        for optimizer in (self.actor, self.critic):
            for parameter, state in optimizer.state.items():
                check(all(state[k].shape == parameter.shape for k in ('exp_avg', 'exp_avg_sq')),
                    'parent-incompatible', 'restore', 'Adam shape/order mismatch')
        check(digest(self.model.state_dict()) == source['model_sha256'],
            'parent-incompatible', 'restore', 'model import changed weights')
        check(digest(self.actor.state_dict()) == digest(source['actor_optimizer'])
            and digest(self.critic.state_dict()) == digest(source['critic_optimizer']),
            'parent-incompatible', 'restore', 'Adam import changed parameter ownership or saved values')
        self.sampler, self.shuffle = torch.Generator(), torch.Generator()
        self.sampler.set_state(source['sampler_rng'])
        self.shuffle.set_state(source['shuffle_rng'])
        self.streams = deepcopy(source['streams'])
        self.ordinal = saved['child_update'] if saved else 0
        self.device = device
        self.buffer = ReplayBuffer(parent['sequences'], imports if arm['import_successes'] else [], spec['strata'],
            saved['buffer'] if saved else None)
        self.behavior = deepcopy(self.model).cpu()
        if not saved:
            check(all(s['native'] is None and s['episode'] == 256 for s in self.streams),
                'parent-incompatible', 'fork', 'parent has a live episode or different episode clock')
        if arm['sampler'] == 'balanced':
            check({s['stratum'] for s in self.buffer.fixed.values()} == {'large', 'small'},
                'stratum-empty', 'fork', 'balanced fork requires both source strata')

    def behavior_copy(self):
        self.behavior.load_state_dict({k: v.detach().cpu() for k, v in self.model.state_dict().items()})
        self.behavior.eval()
        return self.behavior

    def checkpoint(self, streams):
        return dict(schema=CHECKPOINT, protocol_sha256=self.protocol_hash, parent=self.binding, arm=self.arm,
            seed=self.seed, device=self.device, child_update=self.ordinal,
            global_update=self.spec['parent_update'] + self.ordinal,
            model=self.model.state_dict(), model_sha256=digest(self.model.state_dict()),
            actor_optimizer=self.actor.state_dict(), critic_optimizer=self.critic.state_dict(),
            sampler_rng=self.sampler.get_state(), shuffle_rng=self.shuffle.get_state(), streams=streams,
            buffer=self.buffer.snapshot())

    def update(self, payload, evidence, config):
        ordinal = self.ordinal + 1
        sequences = self.buffer.eligible(evidence, ordinal)
        result = update_packed(self.model, self.actor, self.critic, self.shuffle,
            payload['samples'], payload['frontiers'], config['ppo'], sequences,
            self.arm['coefficient'], self.arm['sampler'], self.seed,
            self.spec['parent_update'] + ordinal, ordinal, self.device)
        self.buffer.mark_used(result['success']['selected_ids'])
        evidence.begin(ordinal, payload)
        events = self.buffer.ingest(payload, ordinal, evidence, config['success'], config['ppo']['gamma'])
        self.ordinal = ordinal
        payload.update(seed=self.seed, update=ordinal, global_update=self.spec['parent_update'] + ordinal,
            protocol_sha256=self.protocol_hash, ppo=result)
        return dict(seed=self.seed, update=ordinal, global_update=payload['global_update'],
            actions=len(payload['samples']), area_reward=sum(s['reward'] for s in payload['samples']),
            improved_commits=sum(s['outcome']['status'] == 'improved' for s in payload['samples']),
            valid_commits=sum(s['validated_witness'] is not None for s in payload['samples']),
            completed_episodes=len(payload['completed']), collection_seconds=payload['collection_seconds'],
            quality=training_quality(payload), ppo=result, admission_events=events,
            buffer=self.buffer.summary(sequences))


def publish(child, payload, record, streams, directory, job):
    began = time.perf_counter()
    directory = Path(directory)
    ordinal = child.ordinal
    chunk = directory / f'chunks/{ordinal:04d}.pt.gz'
    checkpoint = directory / f'checkpoints/{ordinal:04d}.pt.gz'
    save(chunk, payload)
    save(checkpoint, child.checkpoint(streams))
    record.update(chunk=reference(chunk, directory), checkpoint=reference(checkpoint, directory))
    record['storage_seconds'] = time.perf_counter() - began
    path = directory / f'updates/{ordinal:04d}.json'
    write(path, record)
    job['records'].append(reference(path, directory))
    job.update(update=ordinal, latest_checkpoint=reference(checkpoint, directory))
    write(directory / 'state.json', job)
    return record


def retain(child, directory, job):
    """Keep success episodes and live buffers, plus selected and recovery chunks."""
    directory = Path(directory)
    keep = set(child.spec['retention']['checkpoints'])
    keep.update(range(max(0, child.ordinal - child.spec['retention']['recovery_updates'] + 1), child.ordinal + 1))
    needed = set(keep)
    for entry in child.buffer.own.state['entries'].values():
        for ordinal, _ in entry['segment']['references']:
            needed.update((ordinal if ordinal % 2 else ordinal - 1, ordinal if ordinal % 2 == 0 else ordinal + 1))
    for active in child.buffer.own.state['active'].values():
        needed.update(ref[0] for ref, _ in active['rows'])
    removed = job.setdefault('removed', [])
    for folder, retained in [('checkpoints', keep), ('chunks', needed)]:
        for path in (directory / folder).glob('*.pt.gz'):
            if int(path.name.split('.')[0]) not in retained:
                removed.append(reference(path, directory))
                path.unlink()
    write(directory / 'state.json', job)
