"""Freeze audited entity policies, select capacity on development, then confirm held-out chains."""

import argparse
from fractions import Fraction
import json
from pathlib import Path
import subprocess
import time

import torch

from entity_learning_curves import run as summarize_curves
from evidence import provenance, write
from geometry_policy_evaluation import admitted_cases
from geometry_restart_confirmation import verify_training_inputs
from geometry_restart_diagnosis import evaluate_cell
from mastery_policy_selection import digest, read
from native import Runtime
from native_training_comparison import policy_hash
from policy import Policy


def width_metrics(cells, parameters):
    if not cells:
        raise ValueError('entity_confirmation.cells: cannot rank an empty development result')
    quality = [sum(Fraction(c['median_score'][0], c['initial_score'][0]) for c in cells)/len(cells)]
    quality.extend(sum(Fraction(c['median_score'][i]) for c in cells)/len(cells) for i in (1, 2))
    return {'passed': all(c['passed'] for c in cells), 'quality_exact': [str(x) for x in quality],
            'parameters': parameters}


def select_width(metrics):
    eligible = [width for width, row in metrics.items() if row['passed']]
    return min(eligible, key=lambda width: (*map(Fraction, metrics[width]['quality_exact']),
                                           metrics[width]['parameters'])) if eligible else None


def freeze(root, source, row, cases, spec, protocol_path, library, output):
    metadata = read(source/'metadata.json')
    verify_training_inputs(root, metadata, cases['train'])
    banks = [(bank, value['episodes']) for value in spec['confirmation'].values()
             if isinstance(value, dict) for bank in value['seed_banks']]
    observed = [(metadata['config']['evaluation_seed_start'], metadata['config']['evaluation_seeds']),
                (metadata['protocol']['assessment']['seed_start'], metadata['protocol']['assessment']['episodes'])]
    if any(max(a, b) < min(a+n, b+m) for a, n in banks for b, m in observed):
        raise ValueError('entity_confirmation.seeds: confirmation overlaps training evaluation')
    config = {**metadata['config'], 'horizon': spec['confirmation']['attempt_horizon'],
              'evaluation_batch_size': 16}
    torch.set_num_threads(config['threads'])
    runtime = Runtime(root, next(iter(cases['train'].values())), config, library)
    current = provenance(runtime, protocol_path)
    original = next(iter(metadata['provenance'].values()))
    if (not current['git_worktree_clean']
            or current['native']['build']['files_sha256'] != original['native']['build']['files_sha256']
            or any(digest(p) != original['files_sha256'][str(p.relative_to(root))]
                   for p in [root/'tools/factory-edit-pilot/policy.py', *runtime.paths.values()])):
        raise ValueError(f'entity_confirmation.source: {source}: changed policy/catalog/native source')
    chosen = row['selection']
    checkpoint = source/f"checkpoint-{chosen['commands']}.pt"
    if digest(checkpoint) != chosen['checkpoint_sha256']:
        raise ValueError(f'entity_confirmation.checkpoint: {checkpoint}: changed retained checkpoint')
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    if saved['metadata']['completed_commands'] != chosen['commands'] or saved['metadata']['seed'] != row['seed']:
        raise ValueError(f'entity_confirmation.checkpoint: {checkpoint}: mismatched checkpoint metadata')
    example, _ = runtime.new_engine().observe()
    actor = Policy(example, normalization=config.get('normalization', 'none'),
                   width_multiplier=row['width']).to(config['device'])
    actor.load_state_dict(saved['evaluation_policy'])
    actor.eval()
    record = {**chosen, 'width': row['width'], 'training_seed': row['seed'],
              'parameters': row['parameters'], 'policy_sha256': policy_hash(actor.state_dict()),
              'metadata_sha256': digest(source/'metadata.json'),
              'audit_sha256': digest(source/'study-audit.json'), 'provenance': current}
    destination = output/f"selected-width{row['width']}-seed{row['seed']}.pt"
    torch.save({'schema_version': 1, 'kind': 'inference_policy', 'policy': actor.state_dict(),
                'config': config, 'dimensions': actor.dimensions, 'source': record}, destination)
    record['export_sha256'] = digest(destination)
    return actor, config, record


def confirm_stages(retained, evaluate, persist):
    """Complete all development cells before choosing which width may see held-out data."""
    development = {width: evaluate('development', width, policies)
                   for width, policies in retained.items()}
    metrics = {width: width_metrics(cells, retained[width][0][2]['parameters'])
               for width, cells in development.items()}
    chosen = select_width(metrics)
    result = {'development': development, 'width_metrics': metrics, 'selected_width': chosen, 'heldout': {}}
    persist(result)
    if chosen is not None:
        result['heldout'][chosen] = evaluate('heldout', chosen, retained[chosen])
        if not result['heldout'][chosen]:
            raise ValueError('entity_confirmation.cells: empty held-out result')
    result['status'] = 'passed' if chosen is not None and all(c['passed'] for c in result['heldout'][chosen]) else 'not_met'
    return result


def run(root, source, library, output):
    protocol_path = root/'tools/factory-edit-pilot/entity-learning-matrix.json'
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('entity_confirmation.source: commit source before confirmation')
    if output.exists():
        raise ValueError(f'entity_confirmation.output: {output}: choose a fresh directory')
    output.mkdir(parents=True)
    started = time.perf_counter()
    result = {'schema_version': 1, 'status': 'running', 'selection': [], 'stages': {},
              'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()}
    stage = 'selection'
    try:
        curves = summarize_curves(source, output/'curves.json')
        spec = curves['protocol']
        result.update(protocol=spec, protocol_sha256=digest(protocol_path), curves_sha256=digest(output/'curves.json'))
        cases = admitted_cases(root, spec)
        retained = {width: [] for width in curves['eligible_widths']}
        for row in curves['runs']:
            if row['width'] not in retained:
                continue
            policy = freeze(root, source/f"width{row['width']}-seed{row['seed']}", row, cases,
                            spec, protocol_path, library, output)
            retained[row['width']].append(policy)
            result['selection'].append(policy[2])
        # Persist every frozen export before opening any development outcome.
        write(output/'confirmation.json', result)

        def persist(stages):
            result['stages'].update(stages)
            write(output/'confirmation.json', result)

        def evaluate(split, width, policies):
            nonlocal stage
            stage = split
            cells = []
            cell_spec = {**spec['confirmation'][split], 'total_commands': spec['confirmation']['total_commands']}
            for actor, config, record in policies:
                seed = record['training_seed']
                for bank in cell_spec['seed_banks']:
                    for key, path in cases[split].items():
                        runtime = Runtime(root, path, config, library)
                        initial = runtime.new_engine().initial_score
                        cell = evaluate_cell(root, runtime, actor, config, cell_spec,
                            spec['confirmation']['attempt_horizon'], bank,
                            output/split/f'width{width}-seed{seed}'/f'bank{bank}'/key)
                        cells.append({'width': width, 'training_seed': seed, 'case': key,
                                      'seed_start': bank, 'initial_score': initial, 'case_training_commands': 0, **cell})
                        if policy_hash(actor.state_dict()) != record['policy_sha256']:
                            raise ValueError(f'entity_confirmation.mutation: width {width} seed {seed}: policy changed')
                        result['stages'].setdefault(split, {})[str(width)] = cells
                        write(output/'confirmation.json', result)
                        print(json.dumps({'stage': split, 'width': width, 'seed': seed,
                                          'case': key, 'bank': bank, 'successes': cell['translated_successes']}), flush=True)
            return cells

        stages = confirm_stages(retained, evaluate, persist)
        result.update(status=stages.pop('status'), stages=stages)
        if result['status'] == 'not_met':
            result['diagnostic'] = {'code': 'entity_confirmation.criteria_not_met', 'severity': 'info',
                'stage': stage, 'message': 'No eligible width or a declared development/held-out reliability criterion failed'}
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'entity_confirmation.interrupted',
                      'severity': 'error', 'stage': stage, 'message': str(error)})
        write(output/'confirmation.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'confirmation.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    result = run(Path(__file__).resolve().parents[2], args.source.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
