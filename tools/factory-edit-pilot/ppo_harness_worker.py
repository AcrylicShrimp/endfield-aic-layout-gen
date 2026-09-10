"""Persistent CPU collection workers with durable result publication."""
import argparse
import json
import os
from pathlib import Path
import signal
import sys
import time

import torch

from native_factory_runtime import FactoryRuntime
from full_factory_on_policy import parameter_hash
from ppo_area_model import models
from ppo_area_update import save_experience
from ppo_harness_state import SCHEMA, require, diagnostic, read, write, sha, owned, check_sources, sync_file, sync_ancestors
from ppo_training_rollout import rollout
from random_factory_generation import generate
from ppo_start_curriculum import materialize, annotate_fresh


def work(root):
    protocol = read(root / 'protocol.json')
    protocol_hash = sha(root / 'protocol.json')
    check_sources(protocol)
    spec, config = protocol['spec'], protocol['config']
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    actor, critic = models(spec, 0)
    runtime, active = None, None
    def interrupted(signum, _):
        raise InterruptedError(f'worker signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
        loaded = None
        print(json.dumps(dict(kind='ready', pid=os.getpid())), flush=True)
        for line in sys.stdin:
            command = json.loads(line)
            if command['op'] == 'stop':
                break
            require(command['op'] == 'collect', 'worker-command', 'expected collect or stop', 'collection')
            active = owned(root, command['task'])
            task = read(active)
            require(task['schema'] == SCHEMA and task['protocol_sha256'] == protocol_hash,
                    'task-protocol', 'task belongs to another protocol', 'collection', active)
            require(task['candidate'] in config['candidates'], 'task-candidate', 'unregistered task input', 'collection', active)
            began = time.monotonic()
            def guard():
                require(time.monotonic() - began < config['limits']['task_seconds'], 'task-timeout',
                        'fresh generation/episode exceeded task budget', 'collection', active)
            if loaded != task['policy_file_sha256']:
                checkpoint = owned(root, task['policy'])
                require(sha(checkpoint) == task['policy_file_sha256'], 'worker-checkpoint-hash',
                        'checkpoint file hash differs', 'collection', checkpoint)
                saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
                require(saved['schema'] == SCHEMA and saved['protocol_sha256'] == protocol_hash and
                        saved['job'] == task['job'], 'worker-checkpoint', 'wrong checkpoint contract', 'collection', checkpoint)
                actor.load_state_dict(saved['policy'])
                critic.load_state_dict(saved['critic'])
                require(parameter_hash(actor) == task['policy_sha256'] and parameter_hash(critic) == task['critic_sha256'],
                        'worker-weights', 'actor/critic checkpoint identity differs', 'collection', checkpoint)
                loaded = task['policy_file_sha256']
            if task.get('initial_start', {}).get('depth', 0):
                case, generation = materialize(root, runtime, spec, task, active.parent / 'generated', guard)
            else:
                case, generation = generate(runtime, spec, task['generation_seed'], active.parent / 'generated', guard)
                if 'initial_start' in task:
                    annotate_fresh(generation, task)
                    write(active.parent / 'generated/generation.json', generation)
            result = dict(schema=SCHEMA, input_sha256=sha(active), identity=task['identity'], candidate=task['candidate'],
                          protocol_sha256=protocol_hash, generation=generation, policy_sha256=task['policy_sha256'],
                          critic_sha256=task['critic_sha256'], pid=os.getpid())
            durable = [active.parent / 'generated' / 'generation.json']
            if case is None:
                result['status'] = 'generation-rejected'
            else:
                selection = dict(spec['selection'], horizon=task['horizon'], reward=config['reward'])
                row, payload = rollout(runtime, dict(config=selection), case, actor, critic, task['candidate'],
                                       task['action_seed'], active.parent / 'episode', task['training'], guard)
                result.update(status='complete', row=row)
                durable.extend(Path(case[k]) for k in ('request', 'initial', 'original'))
                durable.extend(active.parent / 'episode' / name for name in ('episode.json.gz', 'best.json.gz'))
                if payload is not None:
                    path = active.parent / 'experience.pt.gz'
                    payload.update(policy_sha256=task['policy_sha256'], input_sha256=result['input_sha256'])
                    save_experience(path, payload)
                    durable.append(path)
                    result.update(experience=str(path), experience_sha256=sha(path), experience_bytes=path.stat().st_size)
            for path in durable:
                sync_file(path)
                sync_ancestors(path, root)
            result['seconds'] = time.monotonic() - began
            path = active.parent / 'result.json'
            write(path, result)
            envelope = dict(kind='result', path=str(path), sha256=sha(path), task=str(active))
            write(active.parent / 'result-ready.json', envelope)
            print(json.dumps(envelope), flush=True)
            active = None
    except BaseException as error:
        value = diagnostic(error, 'collection', active)
        if active is not None:
            write(active.parent / 'worker-failure.json', dict(status='failed', diagnostic=value))
        print(json.dumps(dict(kind='error', diagnostic=value)), flush=True)
        raise
    finally:
        if runtime is not None:
            runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    work(parser.parse_args().run.resolve())
