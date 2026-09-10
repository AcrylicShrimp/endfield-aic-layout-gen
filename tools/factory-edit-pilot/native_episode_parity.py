"""Explicit complete-factory migration oracle against a frozen reference checkout."""
import argparse
import json
from pathlib import Path
import subprocess
import sys


class ReferenceRuntime:
    """Only this offline oracle starts the historical Node executor."""
    def __init__(self, root, module, catalogs):
        self.module = module
        self.catalog_paths = Path(catalogs).read_text()
        self.worker = subprocess.Popen(['node', str(root / 'tools/factory-edit-pilot/full_factory_executor.mjs'),
            '--catalogs', str(catalogs)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.serial = 0

    def rpc(self, op, witness, **kwargs):
        self.serial += 1
        self.worker.stdin.write(json.dumps(dict(schema_version=3, id=self.serial, op=op, witness=witness, **kwargs)) + '\n')
        self.worker.stdin.flush()
        result = json.loads(self.worker.stdout.readline())
        if result.get('id') != self.serial or result.get('status') == 'error':
            raise RuntimeError(result)
        return result

    def close(self):
        self.worker.stdin.close()
        self.worker.stdin = None
        self.worker.communicate(timeout=10)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--reference-root', type=Path, required=True)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--catalogs', type=Path, required=True)
    parser.add_argument('--case', type=Path, required=True)
    parser.add_argument('--steps', type=int, default=10)
    parser.add_argument('--contracts', nargs='+')
    args = parser.parse_args()
    sys.path.insert(0, str(args.reference_root / 'tools/factory-edit-pilot'))
    from native import load_native
    from full_factory_environment import FactoryEpisode, digest
    from full_factory_input_contracts import CONTRACTS, observation_hash, selected_action_token, history_record
    from full_factory_selection import SelectionMemory, physical_key
    from ppo_training_observation import observation_pair, WitnessIdentities
    import numpy as np
    module = load_native(args.library)
    runtime = module.FullFactoryRuntime.open(str(args.root), args.catalogs.read_text())
    case = json.loads(args.case.read_text())
    reference = ReferenceRuntime(args.reference_root, module, args.catalogs)
    results = []
    try:
        for name in args.contracts or CONTRACTS:
            original = FactoryEpisode(reference, case, args.steps, 300, 5000)
            native = runtime.open_episode(case['request'], case['initial'], json.dumps(dict(
                input_contract=name, horizon=args.steps, elapsed_ms=300000, route_work=5000, memory=True)))
            memory, identities, history = SelectionMemory(original.state['current']), WitnessIdentities(), []
            owned = []
            for ordinal in range(args.steps):
                description = original.describe()
                actual_description = json.loads(native.diagnostic_domain_json())
                for key in ('actions', 'ports', 'chains'):
                    assert actual_description[key] == description[key], (name, ordinal, key)
                keys, allowed, exhausted = identities.mask(original.state, description, memory, True, name)
                actor, critic = observation_pair(original.state, description, 1-ordinal/args.steps, history, name, reference, True)
                view = native.observe(True)
                assert view['allowed'].tolist() == allowed, (name, ordinal, 'mask')
                assert view['memory_exhausted'] == exhausted
                for key, expected in [('actor', actor), ('critic', critic)]:
                    if observation_hash(view[key]) != observation_hash(expected):
                        def compare(path, a, b):
                            if isinstance(a, dict):
                                assert set(a) == set(b), (path, set(a), set(b))
                                for key in a:
                                    compare(f'{path}/{key}', a[key], b[key])
                            elif isinstance(a, np.ndarray):
                                assert a.dtype == b.dtype and a.shape == b.shape, (path, a.shape, b.shape)
                                assert a.tobytes() == b.tobytes(), (path, np.max(np.abs(a-b)))
                            else:
                                assert a == b, (path, a, b)
                        compare(f'{name}/{ordinal}/{key}', view[key], expected)
                        raise AssertionError('unexplained observation hash difference')
                owned.append((view, observation_hash(view['actor']), observation_hash(view['critic'])))
                # Include expansion, contraction, recovery and rejected/local edits.
                kind = ('insert', 'pose', 'reroute', 'contract', 'restore_best', 'component', 'port')[ordinal % 7]
                index = next((i for i,a in enumerate(description['actions']) if allowed[i] and a['kind'] == kind),
                             next(i for i, ok in enumerate(allowed) if ok))
                token = selected_action_token(actor, index)
                before_score = original.state['current_metrics']['score']
                before_area = original.state['best_metrics']['score'][0]
                before_physical = physical_key(original.state['current'])
                expected_step = original.step(description['actions'][index])
                actual_step = native.step(view['handle'], index)
                for field in ('action', 'executor_status', 'status', 'route_expansions', 'current_score', 'best_score', 'unchanged', 'revisited'):
                    assert actual_step[field] == expected_step[field], (name, ordinal, field, actual_step[field], expected_step[field])
                assert actual_step['action_key'] == keys[index], (name, ordinal, 'action_key')
                assert actual_step['physical_after'] == physical_key(original.state['current'])
                assert actual_step['current_sha256'] == digest(original.state['current'])
                assert actual_step['best_sha256'] == digest(original.state['best'])
                assert actual_step['reward'] == (before_area-original.state['best_metrics']['score'][0])/original.state['initial_metrics']['score'][0]
                memory.record(before_physical, physical_key(original.state['current']), keys[index])
                if CONTRACTS[name]['history']:
                    history.append(history_record(token, before_score, expected_step, original.state['initial_metrics']['score']))
                    history = history[-CONTRACTS[name]['history']:]
                try:
                    native.step(view['handle'], index)
                except ValueError as error:
                    assert 'factory.candidate-handle' in str(error)
                else:
                    raise AssertionError('reused handle accepted')
            for view, actor_hash, critic_hash in owned:
                assert observation_hash(view['actor']) == actor_hash
                assert observation_hash(view['critic']) == critic_hash
            results.append(dict(contract=name, steps=native.selected, final_sha256=digest(original.state['current'])))
    finally:
        reference.close()
    print(json.dumps(dict(status='passed', cases=results), indent=2))


if __name__ == '__main__':
    main()
