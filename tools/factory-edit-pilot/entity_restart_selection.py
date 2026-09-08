"""Select all scheduled entity policies under deployment inference, then confirm transfer."""

import argparse
from fractions import Fraction
from pathlib import Path
import subprocess
import time

import torch

from entity_capacity_confirmation import select_width, width_metrics
from entity_learning_curves import run as summarize_curves
from entity_restart_comparison import freeze, identity, quality
from evidence import write
from geometry_policy_evaluation import admitted_cases
from geometry_restart_diagnosis import evaluate_cell
from mastery_policy_selection import digest, read
from native import Runtime
from native_training_comparison import policy_hash


def require_protocol(spec):
    actors = spec['actors']
    expected = {(w, seed, count) for w in (1, 2) for seed in (17, 29, 43)
                for count in range(0, 3145729, 393216)}
    stages = {'selection': (32, 26, [1170000]), 'root_confirmation': (32, 26, [1180000, 1190000]),
              'development': (64, 52, [940000]), 'heldout': (32, 26, [950000, 960000])}
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'deployment_matched_entity_retention'
            or len(actors) != 54 or {(a['width'], a['seed'], a['commands']) for a in actors} != expected
            or any(a['cohort'] != 'scheduled' for a in actors)
            or spec['attempt_horizon'] != 32 or spec['total_commands'] != 128
            or spec['batch_size'] != 16 or spec['device'] != 'cuda' or spec['maximum_seconds'] != 3600
            or spec['expected_native_source_changes'] != ['crates/aic-pilot-native/src/engine.rs']
            or set(spec['stages']) != set(stages)
            or any((spec['stages'][key]['episodes'], spec['stages'][key]['required_successes'],
                    spec['stages'][key]['seed_banks']) != value for key, value in stages.items())):
        raise ValueError('entity_restart_selection.protocol: require complete declared selection and confirmation')


def choose_checkpoints(cells, spec, keys):
    expected = {(identity(a), key, bank) for a in spec['actors'] for key in keys
                for bank in spec['stages']['selection']['seed_banks']}
    actual = {(c['actor'], c['case'], c['seed_start']) for c in cells}
    if len(actual) != len(cells) or actual != expected:
        raise ValueError('entity_restart_selection.matrix: incomplete or duplicate selection grid')
    curves, selections = {}, []
    for width, seed in sorted({(a['width'], a['seed']) for a in spec['actors']}):
        curve = []
        for actor in spec['actors']:
            if (actor['width'], actor['seed']) != (width, seed):
                continue
            selected = [c for c in cells if c['actor'] == identity(actor)]
            if any(c['charged_commands'] != 4096 or c['resets_after_initial'] != 96 for c in selected):
                raise ValueError('entity_restart_selection.accounting: unequal selection command/reset budget')
            curve.append({**actor, 'qualified': all(c['translated_successes'] >= spec['stages']['selection']['required_successes'] for c in selected),
                          'successes': sum(c['translated_successes'] for c in selected),
                          'weakest_case': min(c['translated_successes'] for c in selected),
                          'quality_exact': [str(v) for v in quality(selected)]})
        curve.sort(key=lambda row: row['commands'])
        eligible = [p for p in curve if p['qualified']]
        chosen = min(eligible, key=lambda p: (*map(Fraction, p['quality_exact']), p['commands'])) if eligible else None
        curves[f'width{width}-seed{seed}'] = curve
        selections.append({'width': width, 'seed': seed, 'selection': chosen})
    widths = [w for w in (1, 2) if all(r['selection'] for r in selections if r['width'] == w)]
    return {'curves': curves, 'selections': selections, 'eligible_widths': widths}


def confirm(retained, evaluate, persist):
    result = {'root_confirmation': {}, 'development': {}, 'heldout': {}, 'selected_width': None,
              'confirmation_status': 'not_met', 'failed_stage': 'selection'}
    if not retained:
        return result
    for width, policies in retained.items():
        cells = evaluate('root_confirmation', width, policies)
        result['root_confirmation'][width] = {'cells': cells, **width_metrics(cells, policies[0]['parameters'])}
    persist(result)
    eligible = [w for w, row in result['root_confirmation'].items() if row['passed']]
    if not eligible:
        result['failed_stage'] = 'root_confirmation'
        return result
    for width in eligible:
        cells = evaluate('development', width, retained[width])
        result['development'][width] = {'cells': cells, **width_metrics(cells, retained[width][0]['parameters'])}
    chosen = select_width(result['development'])
    result['selected_width'] = chosen
    persist(result)
    if chosen is None:
        result['failed_stage'] = 'development'
        return result
    cells = evaluate('heldout', chosen, retained[chosen])
    result['heldout'][chosen] = {'cells': cells, **width_metrics(cells, retained[chosen][0]['parameters'])}
    result['confirmation_status'] = 'passed' if result['heldout'][chosen]['passed'] else 'not_met'
    result['failed_stage'] = None if result['confirmation_status'] == 'passed' else 'heldout'
    return result


def run(root, source, reference, protocol_path, library, output):
    spec = read(protocol_path)
    require_protocol(spec)
    if output.exists() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('entity_restart_selection.source: require clean source and fresh output')
    if (digest(source/'matrix.json') != spec['source_matrix_sha256']
            or digest(reference/'comparison.json') != spec['reference_comparison_sha256']
            or not torch.cuda.is_available()):
        raise ValueError('entity_restart_selection.source: changed matrix/comparison or unavailable CUDA runtime')
    previous = read(reference/'comparison.json')
    if previous['status'] != 'complete' or any(r['restart_candidate_for_transfer'] for r in previous['decision']['final_widths'].values()):
        raise ValueError('entity_restart_selection.history: expected complete failed final-actor comparison')
    output.mkdir(parents=True)
    started, active = time.perf_counter(), 'freeze'
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
              'frozen': [], 'selection_cells': [], 'confirmation_cells': [], 'prior_capacity_criterion_passed': False,
              'prior_final_restart_criterion_passed': False}

    def persist():
        write(output/'selection.json', result)

    try:
        curves = summarize_curves(source, output/'historical-curves.json')
        if curves['eligible_widths']:
            raise ValueError('entity_restart_selection.history: expected original failed capacity criterion')
        cases = admitted_cases(root, curves['protocol'])
        if {key: len(value) for key, value in cases.items()} != {'train': 16, 'development': 12, 'heldout': 8}:
            raise ValueError('entity_restart_selection.cases: admitted split dimensions differ')
        if any(spec['stages'][stage] != curves['protocol']['confirmation'][stage] for stage in ('development', 'heldout')):
            raise ValueError('entity_restart_selection.banks: preserve original unopened transfer banks')
        torch.backends.cuda.matmul.allow_tf32 = False
        torch.backends.cudnn.allow_tf32 = False
        torch.backends.cudnn.benchmark = False
        frozen = {}
        for a in spec['actors']:
            original = next(r for r in curves['runs'] if (r['width'], r['seed']) == (a['width'], a['seed']))
            if a['commands'] not in [p['commands'] for p in original['curve']]:
                raise ValueError('entity_restart_selection.schedule: checkpoint is outside original root schedule')
            actor, config, record = freeze(root, source, {**a, 'parameters': original['parameters']}, curves,
                                           cases['train'], spec, protocol_path, library, output)
            key = identity(a)
            path = output/f'{key}-provenance.json'
            write(path, record)
            result['frozen'].append({k: v for k, v in record.items() if k != 'provenance'} |
                                    {'provenance_file': path.name, 'provenance_sha256': digest(path)})
            frozen[key] = actor, {**config, 'horizon': spec['attempt_horizon']}, record
        result['historical_curves_sha256'] = digest(output/'historical-curves.json')
        persist()
        rng = torch.get_rng_state().clone()

        def evaluate(stage, policies):
            nonlocal active
            stage_spec = {**spec['stages'][stage], 'total_commands': spec['total_commands']}
            split = 'train' if stage in ('selection', 'root_confirmation') else stage
            cells = []
            for record in policies:
                key = identity(record)
                actor, config, original = frozen[key]
                for bank in stage_spec['seed_banks']:
                    for case, path in cases[split].items():
                        active = f'{stage}/{key}/bank{bank}/{case}'
                        if time.perf_counter()-started >= spec['maximum_seconds']:
                            raise ValueError('entity_restart_selection.budget: elapsed guard exhausted')
                        runtime = Runtime(root, path, config, library)
                        cell = evaluate_cell(root, runtime, actor, config, stage_spec, spec['attempt_horizon'], bank, output/active)
                        cell = {'stage': stage, 'actor': key, 'width': record['width'], 'seed': record['seed'],
                                'case': case, 'seed_start': bank, 'initial_score': runtime.new_engine().initial_score,
                                'evaluations_sha256': digest(output/active/'evaluations.jsonl'), **cell}
                        cells.append(cell)
                        result['selection_cells' if stage == 'selection' else 'confirmation_cells'].append(cell)
                        if policy_hash(actor.state_dict()) != original['policy_sha256'] or not torch.equal(rng, torch.get_rng_state()):
                            raise ValueError('entity_restart_selection.mutation: policy or global sampling RNG changed')
                        persist()
                        print(f"{active}: {cell['translated_successes']}/{stage_spec['episodes']}", flush=True)
            return cells

        evaluate('selection', result['frozen'])
        selection = choose_checkpoints(result['selection_cells'], spec, list(cases['train']))
        write(output/'frozen-selection.json', selection)
        result['selection'] = selection
        result['frozen_selection_sha256'] = digest(output/'frozen-selection.json')
        persist()
        retained = {width: [next(r for r in result['frozen'] if identity(r) == identity(s['selection']))
                           for s in selection['selections'] if s['width'] == width]
                    for width in selection['eligible_widths']}

        def persist_stages(stages):
            result['stages'] = stages
            persist()

        result['stages'] = confirm(retained, lambda stage, width, policies: evaluate(stage, policies), persist_stages)
        for record in result['frozen']:
            directory = source/f"width{record['width']}-seed{record['seed']}"
            if (digest(directory/f"checkpoint-{record['commands']}.pt") != record['checkpoint_sha256']
                    or digest(output/f'{identity(record)}.pt') != record['export_sha256']
                    or digest(output/record['provenance_file']) != record['provenance_sha256']):
                raise ValueError('entity_restart_selection.mutation: checkpoint, export or provenance changed')
        if (digest(source/'matrix.json') != spec['source_matrix_sha256']
                or digest(reference/'comparison.json') != spec['reference_comparison_sha256']
                or digest(output/'frozen-selection.json') != result['frozen_selection_sha256']):
            raise ValueError('entity_restart_selection.mutation: source or frozen selections changed')
        result.update(status='complete', total_seconds=time.perf_counter()-started,
                      charged_commands=sum(c['charged_commands'] for c in result['selection_cells']+result['confirmation_cells']))
        if result['stages']['confirmation_status'] == 'not_met':
            result['diagnostic'] = {'code': 'entity_restart_selection.criteria_not_met', 'severity': 'info',
                'stage': result['stages']['failed_stage'], 'message': 'A declared complete stage failed; dependent outcomes remain closed'}
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'entity_restart_selection.interrupted', 'severity': 'error',
                      'stage': 'deployment_matched_retention', 'entity': active, 'message': str(error)})
        persist()
        raise
    persist()
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--reference-comparison', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(Path(__file__).resolve().parents[2], args.source.resolve(), args.reference_comparison.resolve(),
        args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
