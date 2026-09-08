"""Replicate a selected training-case tree gain with fresh equal wall allowances."""

import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
import time

import torch

from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest, read
from tree_search import METHODS, search
from tree_search_pilot import freeze, validate_result
from native import Runtime


def summarize(cells, spec):
    expected = {(method, seed) for method in spec['methods']
                for seed in range(spec['sampling_seed_start'], spec['sampling_seed_start']+spec['sampling_seeds'])}
    if (len(cells) != len(expected) or {(row['method'], row['seed']) for row in cells} != expected
            or any(row['case'] != spec['case'] or row['independent_validation']['status'] != 'passed' for row in cells)):
        raise ValueError('tree_wall.matrix: require all declared independently validated cells')
    methods = {}
    for method in spec['methods']:
        rows = [row for row in cells if row['method'] == method]
        ordered = sorted(rows, key=lambda row: row['best_score'])
        methods[method] = {'samples': len(rows), 'median_score': ordered[len(ordered)//2]['best_score'],
            'best_score': ordered[0]['best_score'],
            'translated_successes': sum(row['best_score'][0] < row['initial_score'][0]
                and bool(row['independent_validation']['translated_facilities']) for row in rows),
            'additional_compactions': sum(row['best_score'][0] < spec['additional_compaction_area_below'] for row in rows),
            'simulated_commands': sum(row['simulated_commands'] for row in rows),
            'cell_seconds': sum(row['cell_seconds'] for row in rows),
            'maximum_total_cell_overrun_seconds': max(row['total_cell_overrun_seconds'] for row in rows),
            'termination_counts': dict(Counter(row['termination'] for row in rows))}
    indexed = {(row['method'], row['seed']): row for row in cells}
    pairs = []
    for method, baseline in (('uct', 'random_rollouts'), ('puct', 'policy_rollouts'), ('uct', 'policy_rollouts')):
        outcomes, values = Counter(), []
        for seed in range(spec['sampling_seed_start'], spec['sampling_seed_start']+spec['sampling_seeds']):
            a, b = indexed[(method, seed)], indexed[(baseline, seed)]
            if a['termination'] != 'wall_budget' or b['termination'] != 'wall_budget':
                outcomes['unmatched_wall_allowance'] += 1
                continue
            score, other = a['best_score'], b['best_score']
            outcomes['win' if score < other else 'loss' if score > other else 'tie'] += 1
            values.append({'seed': seed, 'score': score, 'baseline_score': other,
                           'simulated_commands': a['simulated_commands'], 'baseline_commands': b['simulated_commands'],
                           'cell_seconds': a['cell_seconds'], 'baseline_cell_seconds': b['cell_seconds']})
        pairs.append({'method': method, 'baseline': baseline, 'outcomes': dict(outcomes), 'pairs': values})
    return {'methods': methods, 'paired_wall_allowances': pairs,
            'scope': 'post-selected training case; one policy; not held-out generalization'}


def run(root, source, protocol_path, selection_evidence, library, output):
    spec = read(protocol_path)
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'post_selected_training_case_wall_replication'
            or tuple(spec['methods']) != METHODS or type(spec['sampling_seeds']) is not int or spec['sampling_seeds'] < 2
            or type(spec['sampling_seed_start']) is not int or spec['sampling_seed_start'] < 0):
        raise ValueError('tree_wall.protocol: invalid purpose, methods or sampling bank')
    if digest(selection_evidence) != spec['selection_evidence_sha256']:
        raise ValueError(f'tree_wall.selection: {selection_evidence}: original comparison changed')
    basis = read(selection_evidence)
    if (basis['status'] != 'complete' or any(spec[key] != basis['protocol'][key]
            for key in ('width', 'training_seed', 'checkpoint_commands', 'horizon', 'device', 'threads'))
            or set(range(spec['sampling_seed_start'], spec['sampling_seed_start']+spec['sampling_seeds']))
                & set(basis['protocol']['sampling_seeds'])):
        raise ValueError('tree_wall.selection: changed frozen conditions or reused sampling seeds')
    if output.exists():
        raise ValueError(f'tree_wall.output: {output}: choose a fresh directory')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('tree_wall.source: commit source before measuring')
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    if spec['case'] not in cases:
        raise ValueError(f"tree_wall.case: {spec['case']}: selected case is outside training split")
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path), 'cells': []}
    started = time.perf_counter()
    try:
        actor, config, selected = freeze(root, source, spec, cases, library, protocol_path, output)
        if selected['policy_sha256'] != basis['selection']['policy_sha256']:
            raise ValueError('tree_wall.policy: selected frozen tensors differ from original pilot')
        result['selection'] = selected
        write(output/'wall.json', result)
        path = cases[spec['case']]
        result['case_input'] = {'id': spec['case'], 'path': str(path.relative_to(root)), 'sha256': digest(path)}
        for index in range(spec['sampling_seeds']):
            seed = spec['sampling_seed_start']+index
            offset = index % len(spec['methods'])
            methods = spec['methods'][offset:]+spec['methods'][:offset]
            for method in methods:
                cell_started = time.perf_counter()
                runtime = Runtime(root, path, config, library)
                row = search(runtime, method, actor, seed, spec['max_transitions'], spec['max_seconds'],
                             spec['max_nodes'], spec['max_tree_bytes'])
                row['independent_validation'] = validate_result(root, path, config, row)
                elapsed = time.perf_counter()-cell_started
                row.update(case=spec['case'], cell_seconds=elapsed,
                           total_cell_overrun_seconds=max(0, elapsed-spec['max_seconds']))
                for field in ('best', 'best_trace'):
                    destination = output/f'{method}-seed{seed}-{field}.json'
                    write(destination, row.pop(field))
                    row[f'{field}_path'] = destination.name
                    row[f'{field}_sha256'] = digest(destination)
                result['cells'].append(row)
                write(output/'wall.json', result)
                print(json.dumps({'method': method, 'seed': seed, 'score': row['best_score'],
                                  'commands': row['simulated_commands'], 'termination': row['termination']}), flush=True)
        result.update(status='complete', summary=summarize(result['cells'], spec))
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'tree_wall.interrupted', 'severity': 'error',
                      'stage': 'selected_case_wall_comparison', 'message': str(error), 'entity': spec['case']})
        write(output/'wall.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'wall.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--selection-evidence', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    result = run(Path(__file__).resolve().parents[2], args.source.resolve(), args.protocol.resolve(),
                 args.selection_evidence.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
