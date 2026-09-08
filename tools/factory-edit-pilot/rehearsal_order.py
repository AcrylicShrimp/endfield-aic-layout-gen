"""Compare exact-count synchronized and staggered rehearsal continuations."""

import argparse
from collections import Counter
from copy import deepcopy
from pathlib import Path
import subprocess
import time

import torch

from case_curriculum import CaseCurriculum
from case_curriculum_study import load
from clock_intervention import paired_counts
from curriculum import Collector
from evidence import write
from mastery_policy_selection import digest, read
from native_training_comparison import equal, policy_hash, rows
from policy import Policy
from reference.acceptance import Catalog
from retained_tradeoff import COMPARISON
from two_case import study_provenance
from update_size_comparison import assess, branch, quality, validate_rehearsal_orders
from update_stability import evaluate


def orders_for(controller, keys, previous_episodes, cadence):
    original = {key: [] for key in keys}
    for _ in range(32):
        counts = Counter()
        for _ in range(4):
            levels, reasons = controller.schedule(keys, previous_episodes, cadence)
            previous_episodes += len(keys)
            for key, level, reason in zip(keys, levels, reasons):
                if reason == 'rehearsal':
                    original[key].append(level)
                    counts[key] += 1
        if counts != Counter({key: 1 for key in keys}):
            raise ValueError('rehearsal_order.schedule: require one rehearsal per case/update')
    staggered = {key: levels[i % 32:]+levels[:i % 32] for i, (key, levels) in enumerate(original.items())}
    validate_rehearsal_orders(original, controller)
    validate_rehearsal_orders(staggered, controller)
    if any(Counter(original[key]) != Counter(staggered[key]) for key in keys):
        raise ValueError('rehearsal_order.schedule: per-case visit multiset changed')
    return {'synchronized': original, 'staggered': staggered,
            'histograms': {kind: [dict(Counter(levels[i] for levels in table.values())) for i in range(32)]
                           for kind, table in (('synchronized', original), ('staggered', staggered))}}


def decide(branches, seeds, anchor, required):
    indexed = {(b['seed'], b['ordering']): b for b in branches}
    if len(indexed) != len(branches) or set(indexed) != {(s, k) for s in seeds for k in ('synchronized', 'staggered')}:
        raise ValueError('rehearsal_order.matrix: require every declared seed/ordering pair')
    pairs, pooled = [], {'synchronized': [], 'staggered': []}
    for seed in seeds:
        sync, staggered = indexed[seed, 'synchronized'], indexed[seed, 'staggered']
        if any([c['case'] for c in b['roots']] != [c['case'] for c in anchor] for b in (sync, staggered)):
            raise ValueError('rehearsal_order.matrix: root case order differs')
        if any([g['update'] for g in b['assessments']] != [8, 16, 24, 32] for b in (sync, staggered)):
            raise ValueError('rehearsal_order.matrix: short-assessment schedule differs')
        if any(a['initial_score'] != b['initial_score'] or a['initial_score'] != c['initial_score']
               for a, b, c in zip(sync['roots'], staggered['roots'], anchor)):
            raise ValueError('rehearsal_order.matrix: root score identity differs')
        paired = [paired_counts(a['episodes'], b['episodes']) for a, b in zip(staggered['roots'], sync['roots'])]
        pairs.append({'seed': seed,
            'worst_assessment': {b['ordering']: min(sum(g['successes'].values()) for g in b['assessments']) for b in (sync, staggered)},
            'root_successes': {b['ordering']: sum(c['translated_successes'] for c in b['roots']) for b in (sync, staggered)},
            'staggered_vs_synchronized': {key: sum(p[key] for p in paired) for key in ('wins', 'ties', 'losses')}})
        for b in (sync, staggered):
            pooled[b['ordering']].extend(b['roots'])
    q = {key: quality(cells) for key, cells in pooled.items()}
    criteria = {
        'worst_assessment_nonworse': all(p['worst_assessment']['staggered'] >= p['worst_assessment']['synchronized'] for p in pairs),
        'strict_assessment_pairs': sum(p['worst_assessment']['staggered'] > p['worst_assessment']['synchronized'] for p in pairs) >= 2,
        'root_success_nonworse': all(p['root_successes']['staggered'] >= p['root_successes']['synchronized'] for p in pairs),
        'pooled_quality_nonworse': q['staggered'] <= q['synchronized'],
        'every_staggered_case_qualifies': all(c['translated_successes'] >= required for c in pooled['staggered']),
        'anchor_quality_retained': q['staggered'] <= quality(anchor)}
    return {'pairs': pairs, 'criteria': criteria, 'candidate_for_replication': all(criteria.values()),
            'pooled_quality_exact': {key: [str(v) for v in value] for key, value in q.items()},
            'anchor_quality_exact': [str(v) for v in quality(anchor)]}


def verify_schedule(record, directory, orders, current, starts):
    seen, current_counts, setup = {key: [] for key in orders}, Counter(), 0
    for row in rows(directory/'collected.jsonl'):
        s = row['summary']
        if s['start_reason'] == 'rehearsal':
            seen[s['case']].append(s['remaining'])
        else:
            if s['remaining'] != current[s['case']]['level']:
                raise ValueError('rehearsal_order.current: current-stage start changed')
            current_counts[s['case']] += 1
        setup += len(row['prefix'])
    expected_setup = sum(sum(len(starts[key][level]) for level in levels)+96*len(starts[key][current[key]['level']])
                         for key, levels in orders.items())
    if (seen != orders or current_counts != Counter({key: 96 for key in orders})
            or setup != expected_setup or setup != record['validation']['training_setup_commands']):
        raise ValueError('rehearsal_order.accounting: actual schedule or setup differs')
    return {'status': 'passed', 'training_setup_commands': setup, 'case_visit_counts_preserved': True}


def require_protocol(spec):
    order = [(1100000, 'synchronized'), (1100000, 'staggered'), (1110000, 'staggered'),
             (1110000, 'synchronized'), (1120000, 'synchronized'), (1120000, 'staggered')]
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'rehearsal_order_comparison'
            or spec['reference_comparison_sha256'] != COMPARISON or spec['anchor_commands'] != 1179648
            or spec['study_protocol'] != 'tools/factory-edit-pilot/entity-learning-width1-study.json'
            or spec['condition'] != 'entity-mastery-rehearsal' or spec['learning_rate_multiplier'] != .1
            or spec['updates'] != 32 or spec['assessment_updates'] != [8, 16, 24, 32]
            or spec['continuation_seeds'] != [1100000, 1110000, 1120000]
            or [(j['seed'], j['ordering']) for j in spec['run_order']] != order
            or spec['assessment_seed_start'] != 1130000 or spec['sampling_seed_start'] != 1140000
            or spec['episodes'] != 64 or spec['horizon'] != 128 or spec['batch_size'] != 16
            or spec['required_root_successes'] != 52 or spec['maximum_seconds'] != 1200 or spec['device'] != 'cuda'):
        raise ValueError('rehearsal_order.protocol: unsupported controlled comparison')


def run(root, source, reference, protocol_path, library, output):
    spec = read(protocol_path)
    require_protocol(spec)
    if output.exists() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('rehearsal_order.source: require clean checkout and fresh output')
    if not torch.cuda.is_available() or digest(reference/'comparison.json') != COMPARISON:
        raise ValueError('rehearsal_order.source: original CUDA runtime and comparison required')
    previous = read(reference/'comparison.json')
    if previous['status'] != 'complete' or previous['decision']['candidate_for_larger_test'] is not False:
        raise ValueError('rehearsal_order.reference: expected original failed criterion')
    metadata, audit = read(source/'metadata.json'), read(source/'study-audit.json')
    checkpoint = source/f"checkpoint-{spec['anchor_commands']}.pt"
    checkpoint_hash = digest(checkpoint)
    if (metadata['status'] != 'complete' or audit['status'] != 'passed'
            or audit['metadata_sha256'] != digest(source/'metadata.json')
            or checkpoint_hash != audit['checkpoints_sha256'][str(spec['anchor_commands'])]
            or checkpoint_hash != previous['checkpoint_sha256']):
        raise ValueError('rehearsal_order.anchor: audited anchor identity differs')
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    original, base, config, training, groups = load(root, root/spec['study_protocol'], spec['condition'], 43, library)
    if (original != metadata['protocol'] or config != saved['metadata']['config'] or training != saved['metadata']['training_config']
            or not equal(saved['policy'], saved['evaluation_policy']) or len(groups[0]) != 16
            or training['horizon'] != 16 or training['rollout_commands'] != 1024):
        raise ValueError('rehearsal_order.config: original training contract differs')
    torch.set_num_threads(config['threads'])
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    provenance = {}
    for key, runtime in groups[0].items():
        current, before = study_provenance(runtime, root/spec['study_protocol'], base), previous['provenance'][key]
        if (current['native'] != {**before['native'], 'library': str(library)}
                or any(current[k] != before[k] for k in ('python', 'torch', 'numpy', 'platform', 'machine'))
                or any(current['files_sha256'][f'tools/factory-edit-pilot/{name}'] != before['files_sha256'][f'tools/factory-edit-pilot/{name}']
                       for name in ('policy.py', 'curriculum.py', 'case_curriculum.py', 'evaluation_batch.py'))):
            raise ValueError(f'rehearsal_order.provenance: runtime or policy/native inputs differ: {key}')
        provenance[key] = current
    protocol = deepcopy(original)
    protocol['assessment']['seed_start'] = spec['assessment_seed_start']
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
              'checkpoint_sha256': checkpoint_hash, 'provenance': provenance, 'prior_criterion_passed': False, 'branches': []}
    active, started = 'anchor', time.perf_counter()
    try:
        anchor = Policy(next(iter(groups[0].values())).new_engine().observe()[0],
                        normalization=config['normalization'], width_multiplier=1).to('cuda').eval()
        anchor.load_state_dict(saved['policy'])
        anchor_hash = policy_hash(anchor.state_dict())
        catalogs = {key: Catalog(root, runtime.path) for key, runtime in groups[0].items()}
        helper = Collector(groups[1], training, 16, spec['continuation_seeds'][0], True, threaded=False)
        try:
            starts = helper.starts
            controller = CaseCurriculum(starts, saved['metadata']['controller'])
            orders = orders_for(controller, helper.keys, saved['metadata']['episodes'], original['rehearsal_every'])
            write(output/'orders.json', orders)
            result['orders_sha256'] = digest(output/'orders.json')
            result['anchor_assessment'] = assess(anchor, groups[2], catalogs, config, protocol, controller, starts, output, 0)
        finally:
            helper.close()
        result['anchor_roots'] = [evaluate(anchor, runtime, catalogs[key], config, spec, output, key, 'anchor')
                                  for key, runtime in groups[0].items()]
        result['anchor_policy_sha256'] = anchor_hash
        write(output/'comparison.json', result)
        for entry in spec['run_order']:
            if time.perf_counter()-started >= spec['maximum_seconds']:
                raise ValueError('rehearsal_order.budget: elapsed comparison guard exhausted')
            job = {**entry, 'multiplier': spec['learning_rate_multiplier'], 'purpose': 'rehearsal_order_diagnostic',
                   'orders_sha256': result['orders_sha256']}
            active = f"seed{job['seed']}-{job['ordering']}"
            override = orders['staggered'] if job['ordering'] == 'staggered' else None
            record = branch(root, saved, anchor, job, spec, config, training, protocol, groups, catalogs, output/active, override)
            record['schedule_validation'] = verify_schedule(record, output/active, orders[job['ordering']], controller.state, starts)
            write(output/active/'branch.json', record)
            result['branches'].append(record)
            write(output/'comparison.json', result)
        result['decision'] = decide(result['branches'], spec['continuation_seeds'], result['anchor_roots'], spec['required_root_successes'])
        if (digest(checkpoint) != checkpoint_hash or policy_hash(anchor.state_dict()) != anchor_hash
                or digest(reference/'comparison.json') != COMPARISON or digest(output/'orders.json') != result['orders_sha256']):
            raise ValueError('rehearsal_order.final: frozen source or anchor changed')
        result.update(status='complete', seconds=time.perf_counter()-started,
            training_commands=sum(b['validation']['training_commands'] for b in result['branches']),
            assessment_commands=16*(result['anchor_assessment']['episodes']+sum(b['validation']['assessment_episodes'] for b in result['branches'])),
            root_commands=sum(e['counts']['commands'] for c in result['anchor_roots'] for e in c['episodes'])+
                          sum(b['validation']['root_commands'] for b in result['branches']),
            optimizer_steps=sum(u['metrics']['optimizer_steps'] for b in result['branches'] for u in b['updates']))
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'rehearsal_order.interrupted', 'severity': 'error',
                      'stage': 'rehearsal_order_comparison', 'entity': active, 'message': str(error)})
        write(output/'comparison.json', result)
        raise
    write(output/'comparison.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--reference-comparison', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    run(args.root.resolve(), args.source.resolve(), args.reference_comparison.resolve(),
        args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
