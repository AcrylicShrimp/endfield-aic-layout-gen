"""Summarize every audited entity checkpoint and apply the declared retention rule."""

import argparse
from fractions import Fraction
import json
from pathlib import Path

from evidence import write
from entity_learning_matrix import validate_studies
from mastery_policy_selection import digest, read
from native_training_comparison import rows


def checkpoint_metrics(episodes, initial, required):
    cells = {}
    for key, values in episodes.items():
        if not values or initial[key][0] <= 0:
            raise ValueError(f'entity_curves.cell: {key}: empty episodes or invalid initial area')
        ordered = sorted(values, key=lambda row: row['score'])
        median = ordered[len(ordered)//2]['score']
        successes = sum(row['score'][0] < initial[key][0] and bool(row['translated_facilities']) for row in values)
        cells[key] = {'episodes': len(values), 'translated_successes': successes,
                      'translation_frequency': successes/len(values), 'median_score': median,
                      'best_score': ordered[0]['score'], 'initial_score': initial[key],
                      'median_area_ratio': float(Fraction(median[0], initial[key][0]))}
    if not cells:
        raise ValueError('entity_curves.cells: no cases')
    quality = (sum(Fraction(cell['median_score'][0], cell['initial_score'][0]) for cell in cells.values())/len(cells),
               sum(Fraction(cell['median_score'][1]) for cell in cells.values())/len(cells),
               sum(Fraction(cell['median_score'][2]) for cell in cells.values())/len(cells))
    return {'cells': cells, 'eligible': all(cell['translated_successes'] >= required for cell in cells.values()),
            'mean_median_area_ratio': float(quality[0]), 'mean_median_tiles': float(quality[1]),
            'mean_median_turns': float(quality[2]),
            'mean_translation_frequency': sum(cell['translation_frequency'] for cell in cells.values())/len(cells),
            'quality_exact': [str(x) for x in quality]}


def select_checkpoint(curve):
    eligible = [point for point in curve if point['eligible']]
    return min(eligible, key=lambda p: (*map(Fraction, p['quality_exact']), p['commands'])) if eligible else None


def matched_times(runs):
    common = min(run['training_seconds'] for run in runs)
    return [{'budget_seconds': common*fraction, 'fraction': fraction,
             'observations': [{'width': run['width'], 'seed': run['seed'],
                  'point': max((point for point in run['curve'] if point['training_seconds'] <= common*fraction),
                               key=lambda point: point['commands'], default=None)} for run in runs]}
            for fraction in (.25, .5, .75, 1.0)]


def summarize_run(directory, matrix_row, matrix, spec):
    metadata, audit = read(directory/'metadata.json'), read(directory/'study-audit.json')
    if (metadata['status'] != 'complete' or audit['status'] != 'passed'
            or audit['metadata_sha256'] != digest(directory/'metadata.json')
            or digest(directory/'study-audit.json') != matrix_row['audit_sha256']
            or audit['source_commit'] != matrix['source_commit']
            or metadata['seed'] != matrix_row['seed'] or audit['seed'] != metadata['seed']
            or metadata['config']['width_multiplier'] != matrix_row['width']
            or metadata['condition'] != spec['condition']
            or metadata['completed_commands'] != metadata['protocol']['commands']):
        raise ValueError(f'entity_curves.source: {directory}: incomplete or mismatched audited run')
    keys = list(metadata['protocol']['cases'])
    checkpoints = metadata['protocol']['checkpoints']
    initial = {key: audit['cases'][key]['layouts']['initial.json'] for key in keys}
    episodes = {key: rows(directory/key/'evaluations.jsonl') for key in keys}
    summaries = rows(directory/'evaluations.jsonl')
    if [(r['commands'], r['case']) for r in summaries] != [(count, key) for count in checkpoints for key in keys]:
        raise ValueError(f'entity_curves.schedule: {directory}: summary schedule differs')
    expected = [(count, seed) for count in checkpoints for seed in range(metadata['config']['evaluation_seed_start'],
                metadata['config']['evaluation_seed_start']+spec['retention']['episodes'])]
    if any([(r['commands'], r['rollout_seed']) for r in values] != expected for values in episodes.values()):
        raise ValueError(f'entity_curves.schedule: {directory}: episode sampling schedule differs')
    curve = []
    for count in checkpoints:
        selected = {key: [row for row in values if row['commands'] == count] for key, values in episodes.items()}
        metrics = checkpoint_metrics(selected, initial, spec['retention']['required_successes'])
        times = [row['training_seconds'] for row in summaries if row['commands'] == count]
        if any(abs(value-times[0]) > 1e-7 for value in times):
            raise ValueError(f'entity_curves.time: {directory}: checkpoint case times differ')
        curve.append({'commands': count, 'training_seconds': times[0], **metrics})
    chosen = select_checkpoint(curve)
    selection = None
    if chosen:
        count = chosen['commands']
        actual = digest(directory/f'checkpoint-{count}.pt')
        if actual != audit['checkpoints_sha256'][str(count)]:
            raise ValueError(f'entity_curves.checkpoint: {directory}: retained checkpoint differs from audit')
        selection = {'commands': count, 'checkpoint_sha256': actual, 'quality_exact': chosen['quality_exact']}
    return {'width': matrix_row['width'], 'seed': matrix_row['seed'], 'source': str(directory),
            'metadata_sha256': digest(directory/'metadata.json'), 'audit_sha256': digest(directory/'study-audit.json'),
            'evaluation_rows_sha256': {key: digest(directory/key/'evaluations.jsonl') for key in keys},
            'parameters': metadata['parameters'], 'training_seconds': metadata['training_seconds'],
            'training_and_evaluation_wall_seconds': matrix_row['training_and_evaluation_wall_seconds'],
            'audit_wall_seconds': matrix_row['audit_wall_seconds'],
            'seconds': metadata['seconds'], 'collector_seconds': metadata['collector_seconds'],
            'controller': metadata['controller'], 'root_levels': metadata['root_levels'],
            'selection': selection, 'curve': curve}


def run(directory, output):
    matrix = read(directory/'matrix.json')
    root = Path(__file__).resolve().parents[2]
    protocol_path = root/'tools/factory-edit-pilot/entity-learning-matrix.json'
    spec = read(protocol_path)
    if matrix['protocol'] != spec or matrix['protocol_sha256'] != digest(protocol_path):
        raise ValueError(f'entity_curves.protocol: {directory}: matrix differs from frozen protocol')
    studies = {width: read(root/path) for width, path in spec['studies'].items()}
    configs = {width: read(root/study['base_config']) for width, study in studies.items()}
    validate_studies(spec, studies, configs)
    expected = [(r['width'], r['seed']) for r in spec['run_order']]
    if (matrix['status'] != 'complete'
            or [(r['width'], r['seed']) for r in matrix['runs']] != expected
            or len(set(expected)) != len(expected)
            or any(r['status'] != 'complete_and_audited' for r in matrix['runs'])):
        raise ValueError(f'entity_curves.matrix: {directory}: require all declared audited streams')
    if output.exists():
        raise ValueError(f'entity_curves.output: {output}: choose a fresh output file')
    for row in matrix['runs']:
        source = directory/f"width{row['width']}-seed{row['seed']}"
        if (row['protocol_sha256'] != digest(root/spec['studies'][str(row['width'])])
                or read(source/'metadata.json')['protocol'] != studies[str(row['width'])]):
            raise ValueError(f'entity_curves.study: {source}: source differs from declared study')
    runs = [summarize_run(directory/f"width{row['width']}-seed{row['seed']}", row, matrix, spec) for row in matrix['runs']]
    widths = sorted({run['width'] for run in runs})
    result = {'schema_version': 1, 'status': 'complete', 'matrix_sha256': digest(directory/'matrix.json'),
              'source_commit': matrix['source_commit'], 'protocol': spec, 'runs': runs,
              'eligible_widths': [width for width in widths if all(run['selection'] for run in runs if run['width'] == width)],
              'matched_training_times': matched_times(runs)}
    write(output, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result = run(args.source.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'eligible_widths': result['eligible_widths'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
