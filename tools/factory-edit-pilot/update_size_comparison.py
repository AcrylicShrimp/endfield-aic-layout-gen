"""Compare paired short continuations with ordinary and reduced learning rates."""

import argparse
from copy import deepcopy
from fractions import Fraction
import json
from pathlib import Path
import random
import subprocess
import time

import numpy as np
import torch

from case_curriculum import CaseCurriculum, assess_cases
from case_curriculum_audit import replay_assessment
from case_curriculum_study import load
from checkpoint_update_replay import cpu_state, episode_record
from clock_intervention import paired_counts
from curriculum import Collector, FIELDS
from evidence import append, write
from mastery_policy_selection import digest, read
from native import layout, snapshot, trace
from native_training_comparison import equal, policy_hash, rows
from policy import Policy
from prefix_recovery import replay
from reference.acceptance import Catalog
from run import refresh_actor
from two_case import study_provenance
from update_stability import evaluate, measured_update


def optimizer_for(model, saved, rate):
    optimizer = torch.optim.Adam(model.parameters(), lr=rate)
    optimizer.load_state_dict(deepcopy(saved))
    for group in optimizer.param_groups:
        group['lr'] = rate
    if not equal(cpu_state(optimizer.state_dict()['state']), cpu_state(saved['state'])):
        raise ValueError('update_size.optimizer: restored Adam moments differ')
    return optimizer


def quality(cells):
    if not cells or any(cell['initial_score'][0] <= 0 for cell in cells):
        raise ValueError('update_size.quality: nonempty positive-area cells required')
    return (sum(Fraction(cell['median_score'][0], cell['initial_score'][0]) for cell in cells)/len(cells),
            sum(Fraction(cell['median_score'][1]) for cell in cells)/len(cells),
            sum(Fraction(cell['median_score'][2]) for cell in cells)/len(cells))


def decide(branches, seeds):
    indexed = {(row['seed'], row['multiplier']): row for row in branches}
    if len(indexed) != len(branches) or set(indexed) != {(seed, rate) for seed in seeds for rate in (1, 0.1)}:
        raise ValueError('update_size.matrix: require exactly one branch for each seed/rate pair')
    pairs, pooled = [], {1: [], 0.1: []}
    for seed in seeds:
        high, low = indexed[seed, 1], indexed[seed, 0.1]
        if ([cell['case'] for cell in high['roots']] != [cell['case'] for cell in low['roots']]
                or not high['assessments']
                or [a['update'] for a in high['assessments']] != [a['update'] for a in low['assessments']]):
            raise ValueError('update_size.matrix: unmatched cases or assessment schedules')
        worst = {rate: min(sum(a['successes'].values()) for a in row['assessments'])
                 for rate, row in ((1, high), (0.1, low))}
        root_success = {rate: sum(cell['translated_successes'] for cell in row['roots'])
                        for rate, row in ((1, high), (0.1, low))}
        paired = [paired_counts(a['episodes'], b['episodes']) for a, b in zip(low['roots'], high['roots'])]
        pairs.append({'seed': seed, 'worst_assessment': worst, 'root_successes': root_success,
                      'low_vs_high': {key: sum(row[key] for row in paired) for key in ('wins', 'ties', 'losses')}})
        for rate, row in ((1, high), (0.1, low)):
            pooled[rate].extend(row['roots'])
    q = {rate: quality(cells) for rate, cells in pooled.items()}
    criteria = {
        'stability_nonworse': all(p['worst_assessment'][0.1] >= p['worst_assessment'][1] for p in pairs),
        'stability_strict_pairs': sum(p['worst_assessment'][0.1] > p['worst_assessment'][1] for p in pairs) >= 2,
        'root_success_nonworse': all(p['root_successes'][0.1] >= p['root_successes'][1] for p in pairs),
        'root_quality_nonworse': q[0.1] <= q[1]}
    return {'pairs': pairs, 'pooled_quality_exact': {rate: [str(v) for v in values] for rate, values in q.items()},
            'criteria': criteria, 'candidate_for_larger_test': all(criteria.values())}


def require_protocol(spec):
    expected = [(1030000, 1), (1030000, 0.1), (1040000, 0.1), (1040000, 1), (1050000, 1), (1050000, 0.1)]
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'update_size_comparison'
            or spec['anchor_commands'] != 1179648 or spec['training_seed'] != 43
            or spec['updates'] != 32 or spec['assessment_updates'] != [8, 16, 24, 32]
            or spec['continuation_seeds'] != [1030000, 1040000, 1050000]
            or [(r['seed'], r['multiplier']) for r in spec['run_order']] != expected
            or spec['assessment_seed_start'] != 1060000 or spec['sampling_seed_start'] != 1070000
            or spec['horizon'] != 128 or spec['episodes'] != 16 or spec['batch_size'] != 16 or spec['device'] != 'cuda'
            or spec['expected_native_source_changes'] != ['crates/aic-pilot-native/src/engine.rs']):
        raise ValueError('update_size.protocol: unsupported paired comparison')


def assess(actor, runtimes, catalogs, config, protocol, controller, starts, output, index):
    gate = assess_cases(actor, runtimes, protocol, {key: row['level'] for key, row in controller.state.items()}, config)
    setup = 0
    for row in gate['rows']:
        prefix = starts[row['case']][row['level']]
        setup += len(prefix)
        if not equal(replay_assessment(catalogs[row['case']], config, prefix, row, 16), row):
            raise ValueError('update_size.assessment: independent assessment replay differs')
    path = output/f'assessment-{index}.json'
    write(path, gate)
    return {'update': index, 'successes': gate['successes'], 'setup_commands': setup,
            'episodes': len(gate['rows']), 'file': path.name, 'sha256': digest(path)}


def rehearsal_levels(keys, levels, reasons, orders, update):
    if orders is None:
        return levels
    return [orders[key][update-1] if reason == 'rehearsal' else level
            for key, level, reason in zip(keys, levels, reasons)]


def validate_rehearsal_orders(orders, controller):
    if orders is not None and (set(orders) != set(controller.state) or any(
            len(orders[key]) != 32 or any(type(level) is not int or not 1 <= level < state['level']
                                        for level in orders[key]) for key, state in controller.state.items())):
        raise ValueError('update_size.rehearsal_orders: require 32 earlier levels for every frozen case')


def branch(root, saved, anchor, job, spec, config, training, protocol, groups, catalogs, output, rehearsal_orders=None):
    output.mkdir()
    seed, multiplier = job['seed'], job['multiplier']
    torch.manual_seed(seed)
    torch.cuda.manual_seed_all(seed)
    np.random.seed(seed)
    random.seed(seed)
    model = deepcopy(anchor).train()
    actor = deepcopy(anchor).eval()
    settings = {**training, 'learning_rate': config['learning_rate']*multiplier}
    optimizer = optimizer_for(model, saved['optimizer'], settings['learning_rate'])
    collector = Collector(groups[1], settings, 16, seed, True)
    controller = CaseCurriculum(collector.starts, saved['metadata']['controller'])
    rng = np.random.default_rng(seed+100000)
    commands, episodes_count = spec['anchor_commands'], saved['metadata']['episodes']
    result = {**job, 'status': 'running', 'learning_rate': settings['learning_rate'],
              'initial_policy_sha256': policy_hash(model.state_dict()), 'updates': [], 'assessments': [], 'roots': []}
    active, rollout, started = 'setup', None, time.perf_counter()

    def save(path, complete):
        torch.save({'schema_version': 1, 'purpose': 'update_size_diagnostic', **job, 'commands': commands,
            'episodes': episodes_count, 'phase': active, 'config': settings, 'controller': controller.state,
            'policy': model.state_dict(), 'evaluation_policy': actor.state_dict(), 'optimizer': optimizer.state_dict(),
            'numpy_rng': rng.bit_generator.state, 'numpy_legacy_rng': np.random.get_state(), 'python_rng': random.getstate(),
            'torch_rng': torch.get_rng_state(), 'cuda_rng': torch.cuda.get_rng_state_all(),
            'stream_rngs': [g.get_state() for g in collector.generators], 'environments': [snapshot(e) for e in collector.envs],
            'partial_rollout': None if complete else rollout,
            'partial_collection': None if complete else collector.partial,
            'pending_transition': None if complete else collector.pending}, path)

    try:
        validate_rehearsal_orders(rehearsal_orders, controller)
        if result['initial_policy_sha256'] != policy_hash(saved['policy']):
            raise ValueError('update_size.anchor: initial policy changed')
        for update_index in range(1, 33):
            active = f'collection-{update_index}'
            rollout = {key: [] for key in FIELDS}
            for _ in range(4):
                collector.partial, collector.pending, collector.completed_steps = None, None, 0
                levels, reasons = controller.schedule(collector.keys, episodes_count, protocol['rehearsal_every'])
                levels = rehearsal_levels(collector.keys, levels, reasons, rehearsal_orders, update_index)
                part, collected = collector.collect(model, levels)
                for key in FIELDS:
                    rollout[key].extend(part[key])
                for case, env, values, level, reason in zip(collector.keys, collector.envs, collected, levels, reasons):
                    commands += 16
                    episodes_count += 1
                    prefix = collector.starts[case][level]
                    summary = episode_record(case, env, values, episodes_count, commands, level, reason,
                                             controller.state[case]['level'], prefix, 16)
                    append(output/'collected.jsonl', {'summary': summary, 'prefix': prefix, 'trace': trace(env),
                        'layout': layout(env, 'best'), 'samples': {key: value for key, value in values.items() if key != 'observations'}})
            active = f'update-{update_index}'
            metrics = measured_update(model, optimizer, rollout, settings, rng)
            if metrics['optimizer_steps'] != 64:
                raise ValueError('update_size.accounting: wrong optimizer step count')
            refresh_actor(model, actor)
            path = output/f'update-{update_index}.pt'
            save(path, True)
            result['updates'].append({'update': update_index, 'commands': commands, 'metrics': metrics,
                                      'checkpoint': path.name, 'checkpoint_sha256': digest(path)})
            if update_index in spec['assessment_updates']:
                active = f'assessment-{update_index}'
                gate = assess(actor, groups[2], catalogs, config, protocol, controller, collector.starts, output, update_index)
                result['assessments'].append(gate)
                print(json.dumps({'seed': seed, 'multiplier': multiplier, 'update': update_index,
                                  'successes': sum(gate['successes'].values())}), flush=True)
            write(output/'branch.json', result)
        active = 'root_evaluation'
        before = policy_hash(actor.state_dict())
        for case, runtime in groups[0].items():
            result['roots'].append(evaluate(actor, runtime, catalogs[case], config, spec, output, case, 'endpoint'))
        if policy_hash(actor.state_dict()) != before:
            raise ValueError('update_size.actor: root evaluation changed actor')
        active = 'independent_training_replay'
        replayed, setup = 0, 0
        for record in rows(output/'collected.jsonl'):
            replay(catalogs[record['summary']['case']], training, record['prefix'], record['trace'], record['layout'])
            replayed += 1
            setup += len(record['prefix'])
        if replayed != 2048 or commands != spec['anchor_commands']+32768:
            raise ValueError('update_size.accounting: incomplete branch')
        root_episodes = sum(len(cell['episodes']) for cell in result['roots'])
        root_commands = sum(row['counts']['commands'] for cell in result['roots'] for row in cell['episodes'])
        if root_episodes != len(groups[0])*spec['episodes'] or root_commands != root_episodes*spec['horizon']:
            raise ValueError('update_size.accounting: root episode or command count differs')
        result.update(status='complete', total_seconds=time.perf_counter()-started, final_policy_sha256=before,
            validation={'training_episodes': replayed, 'training_commands': 32768, 'training_setup_commands': setup,
                        'assessment_episodes': sum(r['episodes'] for r in result['assessments']),
                        'assessment_setup_commands': sum(r['setup_commands'] for r in result['assessments']),
                        'root_episodes': root_episodes, 'root_commands': root_commands},
            collected_sha256=digest(output/'collected.jsonl'))
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'update_size.interrupted', 'severity': 'error',
                      'stage': 'update_size_comparison', 'entity': f'{seed}/{multiplier}/{active}', 'message': str(error)})
        save(output/'interrupted-state.pt', False)
        write(output/'branch.json', result)
        raise
    finally:
        collector.close()
    write(output/'branch.json', result)
    return result


def run(root, source, protocol_path, library, output):
    spec = read(protocol_path)
    require_protocol(spec)
    if output.exists() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('update_size.source: require clean source and a fresh output directory')
    metadata, audit = read(source/'metadata.json'), read(source/'study-audit.json')
    checkpoint = source/f"checkpoint-{spec['anchor_commands']}.pt"
    checkpoint_hash = digest(checkpoint)
    if (metadata['status'] != 'complete' or audit['status'] != 'passed'
            or audit['metadata_sha256'] != digest(source/'metadata.json')
            or audit['source_commit'] != spec['training_source_commit']
            or checkpoint_hash != audit['checkpoints_sha256'][str(spec['anchor_commands'])]):
        raise ValueError('update_size.audit: anchor source is not the declared audited stream')
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    if (saved['metadata']['completed_commands'] != spec['anchor_commands'] or saved['metadata']['seed'] != 43
            or not equal(saved['policy'], saved['evaluation_policy'])):
        raise ValueError('update_size.anchor: incompatible anchor state')
    original, base, config, training, groups = load(root, root/spec['study_protocol'], spec['condition'], 43, library)
    if (original != metadata['protocol'] or config != saved['metadata']['config'] or training != saved['metadata']['training_config']
            or training['horizon'] != 16 or training['rollout_commands'] != 1024 or len(groups[0]) != 16):
        raise ValueError('update_size.contract: training configuration differs')
    for key in ('move_lengths', 'segment_lengths'):
        if len(set(config[key])) != len(config[key]):
            raise ValueError('update_size.migration: historical length lists must be unique')
    torch.set_num_threads(config['threads'])
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    provenance = {}
    for case, runtime in groups[0].items():
        current, previous = study_provenance(runtime, root/spec['study_protocol'], base), metadata['provenance'][case]
        a, b = current['native']['build']['files_sha256'], previous['native']['build']['files_sha256']
        changed = sorted(key for key in a.keys() | b.keys() if a.get(key) != b.get(key))
        if (changed != spec['expected_native_source_changes'] or current['native']['inputs'] != previous['native']['inputs']
                or digest(root/'tools/factory-edit-pilot/policy.py') != previous['files_sha256']['tools/factory-edit-pilot/policy.py']
                or any(current[key] != previous[key] for key in ('python', 'torch', 'numpy', 'platform', 'machine'))):
            raise ValueError(f'update_size.migration: unexpected policy/native/runtime change: {case}')
        provenance[case] = current
    protocol = deepcopy(original)
    protocol['assessment']['seed_start'] = spec['assessment_seed_start']
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
        'checkpoint_sha256': checkpoint_hash, 'audit_sha256': digest(source/'study-audit.json'), 'provenance': provenance,
        'native_source_migration': spec['expected_native_source_changes'], 'branches': []}
    active, started = 'anchor', time.perf_counter()
    try:
        example, _ = next(iter(groups[0].values())).new_engine().observe()
        anchor = Policy(example, normalization=config['normalization'], width_multiplier=1).to(config['device']).eval()
        anchor.load_state_dict(saved['policy'])
        anchor_hash = policy_hash(anchor.state_dict())
        catalogs = {case: Catalog(root, runtime.path) for case, runtime in groups[0].items()}
        helper = Collector(groups[1], training, 16, spec['continuation_seeds'][0], True, threaded=False)
        try:
            controller = CaseCurriculum(helper.starts, saved['metadata']['controller'])
            result['anchor_assessment'] = assess(anchor, groups[2], catalogs, config, protocol, controller, helper.starts, output, 0)
        finally:
            helper.close()
        result['anchor_roots'] = [evaluate(anchor, runtime, catalogs[case], config, spec, output, case, 'anchor')
                                  for case, runtime in groups[0].items()]
        if policy_hash(anchor.state_dict()) != anchor_hash:
            raise ValueError('update_size.anchor: evaluation changed frozen anchor')
        result['anchor_policy_sha256'] = anchor_hash
        write(output/'comparison.json', result)
        for job in spec['run_order']:
            active = f"seed{job['seed']}-lr{job['multiplier']:g}"
            record = branch(root, saved, anchor, job, spec, config, training, protocol, groups, catalogs, output/active)
            result['branches'].append(record)
            write(output/'comparison.json', result)
        result['decision'] = decide(result['branches'], spec['continuation_seeds'])
        if digest(checkpoint) != checkpoint_hash or policy_hash(anchor.state_dict()) != anchor_hash:
            raise ValueError('update_size.final: original checkpoint changed')
        result.update(status='complete', total_seconds=time.perf_counter()-started,
                      training_commands=196608, assessment_commands=409600, root_commands=229376, optimizer_steps=12288)
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'update_size.interrupted', 'severity': 'error',
            'stage': 'update_size_comparison', 'entity': active, 'message': str(error)})
        write(output/'comparison.json', result)
        raise
    write(output/'comparison.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(args.root.resolve(), args.source.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
