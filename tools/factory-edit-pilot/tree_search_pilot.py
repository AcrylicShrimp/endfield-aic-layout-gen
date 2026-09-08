"""Compare predeclared tree allocation on training cases with a fixed final policy."""

import argparse
import json
from pathlib import Path
import resource
import subprocess
import sys
import time

import torch

from evidence import provenance, write
from geometry_policy_evaluation import admitted_cases
from geometry_restart_confirmation import verify_training_inputs
from mastery_policy_selection import digest, read
from native import Runtime
from native_training_comparison import policy_hash
from policy import Policy
from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize
from tree_search import METHODS, search


def validate_protocol(spec):
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'training_only_tree_allocation'
            or tuple(spec['methods']) != METHODS):
        raise ValueError('tree_pilot.protocol: unsupported schema, purpose or comparison methods')
    for key in ('sampling_seeds', 'transition_budgets'):
        values = spec[key]
        if not values or len(set(values)) != len(values) or any(type(value) is not int or value < 1 for value in values):
            raise ValueError(f'tree_pilot.protocol: {key}: nonempty unique positive integers required')
    for key in ('width', 'checkpoint_commands', 'threads', 'horizon', 'max_nodes', 'max_tree_bytes'):
        if type(spec[key]) is not int or spec[key] < 1:
            raise ValueError(f'tree_pilot.protocol: {key}: positive integer required')
    if not isinstance(spec['max_seconds'], (int, float)) or not 0 < spec['max_seconds'] < float('inf'):
        raise ValueError('tree_pilot.protocol: max_seconds: positive finite allowance required')


def validate_result(root, path, config, result):
    catalog = Catalog(root, path)
    accepted = check(catalog, result['best'])
    if not accepted['valid'] or accepted['score'] != result['best_score']:
        raise ValueError(f'tree_pilot.validation: {path}: invalid selected layout')
    env = Environment(catalog, config)
    trace = result['best_trace']
    if len(trace) > config['horizon']:
        raise ValueError(f'tree_pilot.trace: {path}: selected trace exceeds horizon')
    for row in trace:
        if row['command'] not in env.candidates().to_dicts():
            raise ValueError(f'tree_pilot.trace: {path}: selected command outside independent domain')
        reward, _, outcome = env.step(row['command'])
        if abs(reward-row['reward']) > 1e-10 or any(outcome[key] != row[key] for key in ('applied', 'complete', 'improved')):
            raise ValueError(f'tree_pilot.trace: {path}: independent transition differs')
    if serialize(env.best) != result['best']:
        raise ValueError(f'tree_pilot.trace: {path}: independent retained geometry differs')
    translated = [facility['id'] for facility in catalog.case['facilities']
                  if result['best']['placements'][facility['id']][:2] != facility['pose'][:2]]
    return {'status': 'passed', 'replayed_commands': len(trace), 'translated_facilities': translated}


def freeze(root, source, spec, cases, library, protocol_path, output):
    matrix = read(source/'matrix.json')
    geometry_spec = read(root/spec['geometry_protocol'])
    directory = source/f"width{spec['width']}-seed{spec['training_seed']}"
    metadata, audit = read(directory/'metadata.json'), read(directory/'study-audit.json')
    row = next((r for r in matrix['runs'] if (r['width'], r['seed']) == (spec['width'], spec['training_seed'])), None)
    if (matrix['protocol'] != geometry_spec or matrix['protocol_sha256'] != digest(root/spec['geometry_protocol'])
            or not matrix['source_commit'].startswith(spec['training_source_commit'])
            or row is None or row['status'] != 'complete_and_audited'
            or row['audit_sha256'] != digest(directory/'study-audit.json')
            or metadata['status'] != 'complete' or audit['status'] != 'passed'
            or audit['source_commit'] != matrix['source_commit']
            or audit['metadata_sha256'] != digest(directory/'metadata.json')
            or metadata['seed'] != spec['training_seed'] or audit['seed'] != spec['training_seed']
            or metadata['config']['width_multiplier'] != spec['width']
            or metadata['completed_commands'] != spec['checkpoint_commands']
            or metadata['protocol'] != read(root/geometry_spec['studies'][str(spec['width'])])):
        raise ValueError(f'tree_pilot.source: {directory}: missing or mismatched audited final stream')
    verify_training_inputs(root, metadata, cases)
    config = {**metadata['config'], 'device': spec['device'], 'threads': spec['threads'], 'horizon': spec['horizon']}
    torch.set_num_threads(config['threads'])
    runtime = Runtime(root, next(iter(cases.values())), config, library)
    current = provenance(runtime, protocol_path)
    original = next(iter(metadata['provenance'].values()))
    if (not current['git_worktree_clean']
            or current['native']['build']['files_sha256'] != original['native']['build']['files_sha256']
            or any(digest(path) != original['files_sha256'][str(path.relative_to(root))]
                   for path in [root/'tools/factory-edit-pilot/policy.py', *runtime.paths.values()])):
        raise ValueError(f'tree_pilot.source: {directory}: changed native/catalog/policy source')
    checkpoint = directory/f"checkpoint-{spec['checkpoint_commands']}.pt"
    if digest(checkpoint) != audit['checkpoints_sha256'][str(spec['checkpoint_commands'])]:
        raise ValueError(f'tree_pilot.checkpoint: {checkpoint}: final checkpoint differs from audit')
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    if saved['metadata']['completed_commands'] != spec['checkpoint_commands'] or saved['metadata']['seed'] != spec['training_seed']:
        raise ValueError(f'tree_pilot.checkpoint: {checkpoint}: final checkpoint metadata differs')
    example, _ = runtime.new_engine().observe()
    actor = Policy(example, normalization=config.get('normalization', 'none'), width_multiplier=spec['width']).to(config['device'])
    actor.load_state_dict(saved['evaluation_policy'])
    actor.eval()
    record = {'checkpoint_sha256': digest(checkpoint), 'audit_sha256': digest(directory/'study-audit.json'),
              'metadata_sha256': digest(directory/'metadata.json'), 'matrix_snapshot_sha256': digest(source/'matrix.json'),
              'policy_sha256': policy_hash(actor.state_dict()), 'provenance': current,
              'historical_training_seconds': metadata['training_seconds'], 'parameters': metadata['parameters']}
    destination = output/'frozen-policy.pt'
    torch.save({'schema_version': 1, 'kind': 'inference_policy', 'policy': actor.state_dict(),
                'config': config, 'dimensions': actor.dimensions, 'source': record}, destination)
    record['export_sha256'] = digest(destination)
    return actor, config, record


def run(root, source, protocol_path, library, output):
    spec = read(protocol_path)
    validate_protocol(spec)
    if output.exists():
        raise ValueError(f'tree_pilot.output: {output}: choose a fresh directory')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('tree_pilot.source: commit source before measuring')
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    output.mkdir(parents=True)
    started = time.perf_counter()
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec,
              'protocol_sha256': digest(protocol_path), 'cells': []}
    try:
        actor, config, selected = freeze(root, source, spec, cases, library, protocol_path, output)
        result['selection'] = selected
        write(output/'pilot.json', result)
        for key, path in cases.items():
            for budget in spec['transition_budgets']:
                for index, seed in enumerate(spec['sampling_seeds']):
                    methods = spec['methods'][index:]+spec['methods'][:index]
                    for method in methods:
                        cell_started = time.perf_counter()
                        runtime = Runtime(root, path, config, library)
                        row = search(runtime, method, actor, seed, budget, spec['max_seconds'], spec['max_nodes'], spec['max_tree_bytes'])
                        row['independent_validation'] = validate_result(root, path, config, row)
                        row.update(case=key, transition_budget=budget,
                                   transition_allowance_completed=row['simulated_commands'] == budget,
                                   cell_seconds=time.perf_counter()-cell_started,
                                   process_peak_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*(1 if sys.platform == 'darwin' else 1024))
                        stem = f'{key}-{method}-seed{seed}-budget{budget}'
                        for field in ('best', 'best_trace'):
                            destination = output/f'{stem}-{field}.json'
                            write(destination, row.pop(field))
                            row[f'{field}_path'] = destination.name
                            row[f'{field}_sha256'] = digest(destination)
                        result['cells'].append(row)
                        write(output/'pilot.json', result)
                        print(json.dumps({'case': key, 'method': method, 'seed': seed, 'budget': budget,
                                          'score': row['best_score'], 'termination': row['termination']}), flush=True)
        result['status'] = 'complete'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'tree_pilot.interrupted', 'severity': 'error',
                      'stage': 'training_comparison', 'message': str(error), 'entity': None})
        write(output/'pilot.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'pilot.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    result = run(Path(__file__).resolve().parents[2], args.source.resolve(), args.protocol.resolve(),
                 args.native_library.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
