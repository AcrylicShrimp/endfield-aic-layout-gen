"""Compare frozen final entity policies under equal-command inference procedures."""

import argparse
from fractions import Fraction
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
from native_training_comparison import policy_hash, rows
from policy import Policy


def identity(row):
    return f"width{row['width']}-seed{row['seed']}-commands{row['commands']}"


def require_protocol(spec):
    actors = spec['actors']
    finals = {(r['width'], r['seed']) for r in actors if r['cohort'] == 'final'}
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'frozen_entity_restart_comparison'
            or len(actors) != 7 or len({identity(r) for r in actors}) != 7
            or finals != {(w, s) for w in (1, 2) for s in (17, 29, 43)}
            or any(r['commands'] != 3145728 for r in actors if r['cohort'] == 'final')
            or [(r['width'], r['seed'], r['commands']) for r in actors if r['cohort'] == 'retained'] != [(1, 43, 1179648)]
            or spec['attempt_horizons'] != [128, 32] or spec['total_commands'] != 128
            or spec['episodes'] != 32 or spec['seed_banks'] != [1150000, 1160000]
            or spec['required_successes'] != 26 or spec['batch_size'] != 16
            or spec['device'] != 'cuda' or spec['maximum_seconds'] != 1800
            or spec['expected_native_source_changes'] != ['crates/aic-pilot-native/src/engine.rs']):
        raise ValueError('entity_restart.protocol: require the complete declared frozen comparison')


def quality(cells):
    return [sum(Fraction(c['median_score'][i], c['initial_score'][0] if i == 0 else 1)
                for c in cells)/len(cells) for i in range(3)]


def decide(cells, spec, case_keys):
    expected = {(identity(a), h, bank, key) for a in spec['actors'] for h in spec['attempt_horizons']
                for bank in spec['seed_banks'] for key in case_keys}
    actual = {(c['actor'], c['horizon'], c['seed_start'], c['case']) for c in cells}
    if len(actual) != len(cells) or actual != expected:
        raise ValueError('entity_restart.matrix: missing or duplicate actor/method/bank/case')
    actors = {}
    for actor in spec['actors']:
        key = identity(actor)
        methods = {}
        for h in spec['attempt_horizons']:
            selected = [c for c in cells if c['actor'] == key and c['horizon'] == h]
            if any(c['charged_commands'] != spec['episodes']*spec['total_commands']
                   or c['resets_after_initial'] != spec['episodes']*(spec['total_commands']//h-1)
                   for c in selected):
                raise ValueError('entity_restart.accounting: unequal commands or resets')
            methods[str(h)] = {'qualified': all(c['translated_successes'] >= spec['required_successes'] for c in selected),
                'successes': sum(c['translated_successes'] for c in selected),
                'weakest_cell': min(c['translated_successes'] for c in selected),
                'quality_exact': [str(v) for v in quality(selected)]}
        a, b = methods['128'], methods['32']
        actors[key] = {'cohort': actor['cohort'], 'methods': methods,
            'restart_success_nonworse': b['successes'] >= a['successes'],
            'restart_quality_nonworse': list(map(Fraction, b['quality_exact'])) <= list(map(Fraction, a['quality_exact']))}
    widths = {}
    for width in (1, 2):
        selected = [actors[identity(a)] for a in spec['actors'] if a['width'] == width and a['cohort'] == 'final']
        widths[str(width)] = {'methods_qualified': {str(h): all(a['methods'][str(h)]['qualified'] for a in selected)
                                                  for h in spec['attempt_horizons']},
            'restart_candidate_for_transfer': all(a['methods']['32']['qualified'] and a['restart_success_nonworse']
                                                  and a['restart_quality_nonworse'] for a in selected)}
    return {'actors': actors, 'final_widths': widths, 'prior_capacity_criterion_passed': False}


def freeze(root, source, record, curves, cases, spec, protocol_path, library, output):
    directory = source/f"width{record['width']}-seed{record['seed']}"
    metadata, audit = read(directory/'metadata.json'), read(directory/'study-audit.json')
    checkpoint = directory/f"checkpoint-{record['commands']}.pt"
    if (digest(checkpoint) != record['checkpoint_sha256']
            or audit['checkpoints_sha256'][str(record['commands'])] != record['checkpoint_sha256']):
        raise ValueError(f'entity_restart.checkpoint: {checkpoint}: audited hash differs')
    if record['cohort'] == 'retained':
        original = next(r['selection'] for r in curves['runs'] if r['width'] == record['width'] and r['seed'] == record['seed'])
        if original is None or original['checkpoint_sha256'] != record['checkpoint_sha256']:
            raise ValueError('entity_restart.selection: supplemental policy differs from original retention')
    verify_training_inputs(root, metadata, cases)
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    if (saved['metadata']['completed_commands'] != record['commands'] or saved['metadata']['seed'] != record['seed']
            or saved['metadata']['config'] != metadata['config']):
        raise ValueError(f'entity_restart.checkpoint: {checkpoint}: metadata differs')
    config = {**metadata['config'], 'device': spec['device'], 'evaluation_batch_size': spec['batch_size']}
    for key in ('move_lengths', 'segment_lengths'):
        if len(set(config[key])) != len(config[key]):
            raise ValueError(f'entity_restart.migration: {checkpoint}: duplicate historical {key}')
    torch.set_num_threads(config['threads'])
    evidence = {}
    for key, path in cases.items():
        runtime = Runtime(root, path, config, library)
        current, previous = provenance(runtime, protocol_path), metadata['provenance'][key]
        a, b = current['native']['build']['files_sha256'], previous['native']['build']['files_sha256']
        changed = sorted(k for k in a.keys() | b.keys() if a.get(k) != b.get(k))
        if (changed != spec['expected_native_source_changes'] or current['native']['inputs'] != previous['native']['inputs']
                or current['files_sha256']['tools/factory-edit-pilot/policy.py'] != previous['files_sha256']['tools/factory-edit-pilot/policy.py']
                or any(current[k] != previous[k] for k in ('python', 'torch', 'numpy', 'platform', 'machine'))):
            raise ValueError(f'entity_restart.migration: {checkpoint}/{key}: undeclared native/policy/runtime change')
        evidence[key] = current
    example, _ = runtime.new_engine().observe()
    actor = Policy(example, normalization=config['normalization'], width_multiplier=record['width']).to(spec['device']).eval()
    actor.load_state_dict(saved['evaluation_policy'])
    frozen = {**record, 'policy_sha256': policy_hash(actor.state_dict()), 'provenance': evidence,
              'metadata_sha256': digest(directory/'metadata.json'), 'audit_sha256': digest(directory/'study-audit.json')}
    export = output/f'{identity(record)}.pt'
    torch.save({'schema_version': 1, 'kind': 'inference_policy', 'policy': actor.state_dict(),
                'config': config, 'dimensions': actor.dimensions, 'source': frozen}, export)
    frozen['export_sha256'] = digest(export)
    return actor, config, frozen


def paired_results(output, cells, spec, keys):
    result = {}
    for a in spec['actors']:
        actor = identity(a)
        counts = dict(wins=0, ties=0, losses=0)
        for bank in spec['seed_banks']:
            for key in keys:
                paths = [output/actor/f'horizon{h}'/f'bank{bank}'/key/'evaluations.jsonl' for h in (32, 128)]
                restart, continuous = map(rows, paths)
                seeds = list(range(bank, bank+spec['episodes']))
                if any([r['rollout_seed'] for r in group] != seeds for group in (restart, continuous)):
                    raise ValueError('entity_restart.pairing: sampling seed schedule differs')
                for x, y in zip(restart, continuous):
                    counts['wins' if x['score'] < y['score'] else 'losses' if x['score'] > y['score'] else 'ties'] += 1
        result[actor] = counts
    return result


def run(root, source, protocol_path, library, output):
    spec = read(protocol_path)
    require_protocol(spec)
    if output.exists() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('entity_restart.source: require clean source and fresh output')
    if digest(source/'matrix.json') != spec['source_matrix_sha256'] or not torch.cuda.is_available():
        raise ValueError('entity_restart.source: changed source matrix or unavailable original CUDA runtime')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
              'frozen': [], 'cells': []}
    started, active = time.perf_counter(), 'freeze'
    try:
        curves = summarize_curves(source, output/'curves.json')
        if curves['eligible_widths']:
            raise ValueError('entity_restart.history: expected original failed capacity gate')
        cases = admitted_cases(root, curves['protocol'])['train']
        if len(cases) != 16:
            raise ValueError('entity_restart.cases: require all sixteen original training cases')
        torch.backends.cuda.matmul.allow_tf32 = False
        torch.backends.cudnn.allow_tf32 = False
        torch.backends.cudnn.benchmark = False
        retained = [freeze(root, source, a, curves, cases, spec, protocol_path, library, output) for a in spec['actors']]
        result['frozen'] = [r[2] for r in retained]
        result['curves_sha256'] = digest(output/'curves.json')
        write(output/'comparison.json', result)
        rng = torch.get_rng_state().clone()
        for i, (actor, config, record) in enumerate(retained):
            actor_key = identity(record)
            for horizon in spec['attempt_horizons'][::1 if i % 2 == 0 else -1]:
                cell_config = {**config, 'horizon': horizon}
                for bank in spec['seed_banks']:
                    for key, path in cases.items():
                        active = f'{actor_key}/horizon{horizon}/bank{bank}/{key}'
                        if time.perf_counter()-started >= spec['maximum_seconds']:
                            raise ValueError('entity_restart.budget: comparison elapsed guard exhausted')
                        runtime = Runtime(root, path, cell_config, library)
                        directory = output/active
                        cell = evaluate_cell(root, runtime, actor, cell_config, spec, horizon, bank, directory)
                        result['cells'].append({'actor': actor_key, 'horizon': horizon, 'seed_start': bank, 'case': key,
                            'initial_score': runtime.new_engine().initial_score, 'evaluations_sha256': digest(directory/'evaluations.jsonl'), **cell})
                        if policy_hash(actor.state_dict()) != record['policy_sha256'] or not torch.equal(rng, torch.get_rng_state()):
                            raise ValueError('entity_restart.mutation: policy or global sampling RNG changed')
                        write(output/'comparison.json', result)
                        print(f"{active}: {cell['translated_successes']}/{spec['episodes']}", flush=True)
        result['decision'] = decide(result['cells'], spec, list(cases))
        result['paired_restart_vs_continuous'] = paired_results(output, result['cells'], spec, list(cases))
        for record in result['frozen']:
            directory = source/f"width{record['width']}-seed{record['seed']}"
            if (digest(directory/f"checkpoint-{record['commands']}.pt") != record['checkpoint_sha256']
                    or digest(output/f'{identity(record)}.pt') != record['export_sha256']):
                raise ValueError('entity_restart.mutation: input or exported checkpoint changed')
        if digest(source/'matrix.json') != spec['source_matrix_sha256']:
            raise ValueError('entity_restart.mutation: source matrix changed')
        result.update(status='complete', total_seconds=time.perf_counter()-started,
                      charged_commands=sum(c['charged_commands'] for c in result['cells']))
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'entity_restart.interrupted', 'severity': 'error',
                      'stage': 'frozen_inference_comparison', 'entity': active, 'message': str(error)})
        write(output/'comparison.json', result)
        raise
    write(output/'comparison.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(Path(__file__).resolve().parents[2], args.source.resolve(), args.protocol.resolve(),
        args.native_library.resolve(), args.output.resolve())
