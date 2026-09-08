"""Summarize all frozen training-only search cells without claiming wall-time matching."""

import argparse
from collections import Counter
from fractions import Fraction
import json
from pathlib import Path

from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest, read


def method_metrics(cells):
    cases = {}
    for key in sorted({row['case'] for row in cells}):
        values = [row for row in cells if row['case'] == key]
        ordered = sorted(values, key=lambda row: row['best_score'])
        median = ordered[len(ordered)//2]['best_score']
        successes = sum(row['best_score'][0] < row['initial_score'][0]
                        and bool(row['independent_validation']['translated_facilities']) for row in values)
        cases[key] = {'median_score': median, 'best_score': ordered[0]['best_score'],
                      'initial_score': values[0]['initial_score'], 'translated_successes': successes,
                      'samples': len(values), 'completed_allowances': sum(row['transition_allowance_completed'] for row in values)}
    if not cases:
        raise ValueError('tree_summary.cells: cannot summarize empty result')
    ratio = sum(Fraction(cell['median_score'][0], cell['initial_score'][0]) for cell in cases.values())/len(cases)
    return {'cases': cases, 'mean_median_area_ratio_exact': str(ratio),
            'mean_median_area_ratio': float(ratio),
            'mean_median_tiles': sum(cell['median_score'][1] for cell in cases.values())/len(cases),
            'mean_median_turns': sum(cell['median_score'][2] for cell in cases.values())/len(cases),
            'translated_successes': sum(cell['translated_successes'] for cell in cases.values()),
            'samples': len(cells), 'cell_seconds': sum(row['cell_seconds'] for row in cells),
            'simulated_commands': sum(row['simulated_commands'] for row in cells),
            'termination_counts': dict(Counter(row['termination'] for row in cells))}


def paired(cells, method, baseline):
    outcomes = Counter()
    differences = []
    keys = {(row['case'], row['seed']) for row in cells}
    indexed = {(row['case'], row['seed'], row['method']): row for row in cells}
    for case, seed in sorted(keys):
        a, b = indexed[(case, seed, method)], indexed[(case, seed, baseline)]
        if not a['transition_allowance_completed'] or not b['transition_allowance_completed']:
            outcomes['unmatched_allowance'] += 1
            continue
        score_a, score_b = a['best_score'], b['best_score']
        outcomes['win' if score_a < score_b else 'loss' if score_a > score_b else 'tie'] += 1
        differences.append({'case': case, 'seed': seed, 'area_difference': score_a[0]-score_b[0],
                            'score': score_a, 'baseline_score': score_b,
                            'seconds': a['cell_seconds'], 'baseline_seconds': b['cell_seconds']})
    return {'method': method, 'baseline': baseline, 'outcomes': dict(outcomes),
            'pairs': differences, 'wall_time_matched': False}


def run(source, output):
    result = read(source/'pilot.json')
    spec = result['protocol']
    root = Path(__file__).resolve().parents[2]
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    expected = {(case, method, seed, budget) for case in cases for method in spec['methods']
                for seed in spec['sampling_seeds'] for budget in spec['transition_budgets']}
    cells = result['cells']
    if (result['status'] != 'complete' or len(cells) != len(expected)
            or {(r['case'], r['method'], r['seed'], r['transition_budget']) for r in cells} != expected
            or digest(source/'frozen-policy.pt') != result['selection']['export_sha256']):
        raise ValueError(f'tree_summary.source: {source}: require complete matrix and unchanged frozen policy')
    if output.exists():
        raise ValueError(f'tree_summary.output: {output}: choose a fresh file')
    initial = {}
    for row in cells:
        if (row['executed_commands'] != 0 or row['result_origin'] != 'hypothetical_exploration'
                or not row['native_validation']['valid'] or row['independent_validation']['status'] != 'passed'
                or row['native_validation']['score'] != row['best_score']
                or not 0 <= row['simulated_commands'] <= row['transition_budget']
                or row['transition_allowance_completed'] != (row['simulated_commands'] == row['transition_budget'])
                or row['initial_score'] != initial.setdefault(row['case'], row['initial_score'])):
            raise ValueError(f"tree_summary.cell: {row['case']}: invalid score, validation or work accounting")
        for field in ('best', 'best_trace'):
            path = (source/row[f'{field}_path']).resolve()
            if not path.is_relative_to(source.resolve()) or digest(path) != row[f'{field}_sha256']:
                raise ValueError(f'tree_summary.artifact: {path}: selected evidence changed')
    summary = {'schema_version': 1, 'status': 'complete', 'purpose': spec['purpose'],
               'pilot_sha256': digest(source/'pilot.json'), 'selection': result['selection'],
               'total_seconds': result['total_seconds'], 'budgets': {}}
    for budget in spec['transition_budgets']:
        values = [row for row in cells if row['transition_budget'] == budget]
        summary['budgets'][str(budget)] = {
            'methods': {method: method_metrics([row for row in values if row['method'] == method]) for method in spec['methods']},
            'paired_tree_allocation': [paired(values, 'uct', 'random_rollouts'), paired(values, 'puct', 'policy_rollouts')]}
    write(output, summary)
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = run(args.source.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
