"""Measure native branching and full snapshot costs on training geometry only."""

import argparse
from collections import defaultdict
import json
from pathlib import Path
import random
import subprocess
import time

from evidence import provenance, write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import read
from native import Runtime


def probe(runtime, branches, depth, seed, seconds, max_snapshot_bytes):
    if (any(type(value) is not int or value < 1 for value in (branches, depth, max_snapshot_bytes))
            or not isinstance(seconds, (int, float)) or not 0 < seconds < float('inf')
            or type(seed) is not int or seed < 0):
        raise ValueError('tree_cost.budget: positive finite budgets and nonnegative integer seed required')
    root, engine = runtime.new_engine(), runtime.new_engine()
    original = root.export_state_json()
    times, counts = defaultdict(float), defaultdict(int)
    branching, sizes = [], []
    rng = random.Random(seed)
    started = time.perf_counter()
    reason = 'completed'

    def measured(name, function, *args):
        before = time.perf_counter()
        value = function(*args)
        times[name] += time.perf_counter()-before
        counts[name] += 1
        return value

    for _ in range(branches):
        if reason != 'completed':
            break
        measured('restore', engine.restore_state_json, original)
        for _ in range(depth):
            if time.perf_counter()-started >= seconds:
                reason = 'wall_budget'
                break
            saved = measured('snapshot', engine.export_state_json)
            size = len(saved.encode())
            sizes.append(size)
            if size > max_snapshot_bytes:
                reason = 'snapshot_byte_budget'
                break
            # Restoring the whole state invalidates handles and rolls back native counters.
            # Work accounting therefore stays in this caller, outside every snapshot.
            measured('restore', engine.restore_state_json, saved)
            handle, count = measured('candidates', engine.candidates)
            branching.append(count)
            if not count:
                raise ValueError('tree_cost.candidates: active search state has no candidates')
            measured('observation', engine.observe_candidates, handle)
            _, done, _ = measured('step', engine.step, handle, rng.randrange(count))
            if done:
                break
    if root.export_state_json() != original:
        raise ValueError('tree_cost.root_mutation: hypothetical work changed original engine')
    # This cost-only probe exports no hypothetical solution or quality claim.
    return {'termination': reason, 'elapsed_seconds': time.perf_counter()-started,
            'executed_commands': 0, 'simulated_commands': counts['step'], 'counts': dict(counts),
            'seconds': dict(times), 'branching': branching, 'snapshot_bytes': sizes,
            'original_engine_unchanged': True, 'model_calls': 0,
            'memory_scope': 'maximum serialized snapshot payload; not process RSS or a tree-memory limit'}


def run(root, protocol_path, library, output):
    spec = read(protocol_path)
    matrix = read(root/spec['geometry_protocol'])
    cases = admitted_cases(root, matrix)['train']
    if spec.get('schema_version') != 1 or spec.get('purpose') != 'training_only_snapshot_cost':
        raise ValueError('tree_cost.protocol: unsupported schema or purpose')
    if output.exists():
        raise ValueError(f'tree_cost.output: {output}: choose a fresh file')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('tree_cost.source: commit source before measuring')
    config = {**read(root/spec['environment_config']), 'horizon': spec['depth']}
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'cases': {}}
    try:
        for key, path in cases.items():
            runtime = Runtime(root, path, config, library)
            result['cases'][key] = {**probe(runtime, spec['branches'], spec['depth'], spec['seed'],
                spec['max_seconds_per_case'], spec['max_snapshot_bytes']), 'provenance': provenance(runtime, protocol_path)}
            write(output, result)
            print(json.dumps({'case': key, 'simulated_commands': result['cases'][key]['simulated_commands'],
                              'termination': result['cases'][key]['termination']}), flush=True)
        result['status'] = 'complete'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'tree_cost.interrupted', 'severity': 'error',
                      'stage': 'snapshot_cost', 'message': str(error), 'entity': None})
        write(output, result)
        raise
    write(output, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = run(Path(__file__).resolve().parents[2], args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
