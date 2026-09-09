"""Persistent PPO actor/critic workers retaining immutable sampled experience."""
import argparse
import json
import os
from pathlib import Path
import signal
import sys
import time

import torch

from full_factory_environment import FactoryRuntime
from full_factory_input_contracts import InputContractPolicy
from full_factory_ppo_learning import rollout
from ppo_area_model import AreaCritic, SCHEMA
from ppo_area_update import save_experience
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_collection import check_sources
from random_factory_generation import generate, read, write


def worker(args):
    runtime, active = None, None
    def interrupted(signum, _):
        raise InterruptedError(f'ppo-area.worker-interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    try:
        protocol = read(args.protocol)
        check_sources(protocol)
        spec = protocol['spec']
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        torch.use_deterministic_algorithms(True)
        model = InputContractPolicy(spec['policy']['depth'], spec['policy']['width'])
        critic = AreaCritic(spec['policy']['depth'], spec['policy']['width'])
        runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
        loaded = None
        print(json.dumps(dict(kind='ready', pid=os.getpid())), flush=True)
        for line in sys.stdin:
            command = json.loads(line)
            if command.get('op') == 'stop':
                break
            active = Path(command['task'])
            task = read(active)
            began = time.monotonic()
            def guard():
                if time.monotonic() - began >= spec['limits']['task_seconds']:
                    raise TimeoutError(f'ppo-area.task-budget: {active}')
            if task['policy_file_sha256'] != loaded:
                if sha(task['policy']) != task['policy_file_sha256']:
                    raise ValueError('ppo-area.policy-file-identity')
                saved = torch.load(task['policy'], map_location='cpu', weights_only=False)
                if saved['schema'] != SCHEMA or saved['critic_sha256'] != task['critic_sha256']:
                    raise ValueError('ppo-area.behavior-mismatch: checkpoint schema/critic')
                model.load_state_dict(saved['policy'], strict=True)
                critic.load_state_dict(saved['critic'], strict=True)
                loaded = task['policy_file_sha256']
                if parameter_hash(model) != task['policy_sha256'] or parameter_hash(critic) != saved['critic_sha256']:
                    raise ValueError('ppo-area.policy-identity')
            case, generation = generate(runtime, spec, task['generation_seed'], active.parent / 'generated', guard)
            result = dict(input_sha256=sha(active), identity=task['identity'], generation=generation,
                          policy_sha256=parameter_hash(model), critic_sha256=parameter_hash(critic), pid=os.getpid())
            if case is None:
                result.update(status='generation-rejected')
            else:
                selection = dict(spec['selection'], horizon=task['horizon'])
                row, payload = rollout(runtime, dict(config=selection), case, model, critic,
                    'action-only', task['action_seed'], active.parent / 'episode', task['training'], guard)
                result.update(status='complete', row=row)
                if payload is not None:
                    path = active.parent / 'experience.pt.gz'
                    payload.update(policy_sha256=parameter_hash(model), input_sha256=sha(active))
                    save_experience(path, payload)
                    result.update(experience=str(path), experience_sha256=sha(path), experience_bytes=path.stat().st_size)
            result['seconds'] = time.monotonic() - began
            write(active.parent / 'result.json', result)
            print(json.dumps(dict(kind='result', path=str(active.parent / 'result.json'),
                                  sha256=sha(active.parent / 'result.json'))), flush=True)
            active = None
        check_sources(protocol)
    except BaseException as error:
        diagnostic = dict(status='incomplete', code='ppo-area.worker-failed', severity='error',
                          stage='experience-collection', entity=str(active), message=str(error))
        if active is not None:
            write(active.parent / 'partial.json', diagnostic)
        print(json.dumps(dict(kind='error', diagnostic=diagnostic)), flush=True)
        raise
    finally:
        if runtime is not None:
            runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    execute = sub.add_parser('worker')
    execute.add_argument('--protocol', type=Path, required=True)
    worker(parser.parse_args())
