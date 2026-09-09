"""Persistent workers for direct high-water area-reward experience."""
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
from full_factory_area_learning import rollout
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_collection import check_sources
from random_factory_generation import generate, read, write
from direct_area_reward import payload_hash


def worker(args):
    runtime, active = None, None
    def interrupted(signum, _):
        raise InterruptedError(f'random-collection.worker-interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    try:
        protocol = read(args.protocol)
        check_sources(protocol)
        spec = protocol['spec']
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        torch.use_deterministic_algorithms(True)
        model = InputContractPolicy(spec['policy']['depth'], spec['policy']['width'])
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
                    raise TimeoutError(f'random-collection.task-budget: {active}')
            if task['policy_sha256'] != loaded:
                if sha(task['policy']) != task['policy_file_sha256']:
                    raise ValueError('random-collection.policy-file-identity')
                saved = torch.load(task['policy'], map_location='cpu', weights_only=False)
                model.load_state_dict(saved['policy'], strict=True)
                loaded = parameter_hash(model)
                if loaded != task['policy_sha256']:
                    raise ValueError('random-collection.policy-identity')
            case, generation = generate(runtime, spec, task['generation_seed'], active.parent / 'generated', guard)
            result = dict(input_sha256=sha(active), identity=task['identity'], generation=generation,
                          policy_sha256=loaded, pid=os.getpid())
            if case is None:
                result.update(status='generation-rejected')
            else:
                selection = dict(spec['selection'], horizon=task['horizon'])
                row, gradients = rollout(runtime, dict(config=selection), case, model,
                    'action-only', task['action_seed'], active.parent / 'episode', task['training'], guard)
                result.update(status='complete', row=row)
                if gradients is not None:
                    path = active.parent / 'gradients.pt'
                    torch.save(dict(policy_sha256=loaded, input_sha256=sha(active), gradients=gradients), path)
                    result.update(gradients=str(path), gradients_sha256=sha(path),
                        gradient_values_sha256=payload_hash(gradients))
            result['seconds'] = time.monotonic() - began
            write(active.parent / 'result.json', result)
            print(json.dumps(dict(kind='result', path=str(active.parent / 'result.json'),
                                  sha256=sha(active.parent / 'result.json'))), flush=True)
            active = None
        check_sources(protocol)
    except BaseException as error:
        diagnostic = dict(status='incomplete', code='random-collection.worker-failed', severity='error',
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
