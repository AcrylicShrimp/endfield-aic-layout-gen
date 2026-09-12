"""Registered paired endpoints and common geometry-transfer eligibility."""
from collections import Counter
from pathlib import Path
import statistics

from blueprint_ppo_store import checked, read, sha, write
from blueprint_replay_contract import check
from blueprint_replay_panel import identities


def mean(values):
    return statistics.mean(values)


def panel_values(rows):
    grouped = {}
    for row in rows:
        grouped.setdefault(row['generation_seed'], []).append(row['total_area_reduction_percent'])
    return {str(key): mean(values) for key, values in grouped.items()}


def build_report(matrix, output):
    matrix = Path(matrix).resolve()
    launch, state = read(matrix / 'launch.json'), read(matrix / 'state.json')
    check(state['status'] == 'complete', 'source-invalid', 'report', 'matrix is not complete')
    panel = read(checked(launch['transfer_panel'], matrix))
    input_root = Path(launch['training_input_directory'])
    inputs = sorted(input_root.glob('episode-*/episode.json'))
    check(len(inputs) == 512, 'input-namespace', 'report', 'actual continuation input bank is incomplete')
    exact, geometry, products = set(), set(), Counter()
    for path in inputs:
        row = read(path)
        payload = read(checked(row['files']['input'], input_root))
        e, g = identities(payload['original'])
        exact.add(e)
        geometry.add(g)
        products[row['draw']['product']] += 1
    eligibility = [dict(index=i, generation_seed=c['generation_seed'], product=c['product'],
        eligible=c['exact_identity'] not in exact and c['geometry_identity'] not in geometry,
        exact_overlap=c['exact_identity'] in exact, geometry_overlap=c['geometry_identity'] in geometry)
        for i, c in enumerate(panel['cases'])]
    eligible = {str(c['generation_seed']) for c in eligibility if c['eligible']}
    # Historical source generation metadata carries the original product labels.
    historical_path = next(Path(p) for p in launch['external_sources'] if Path(p).name == 'cohort.json')
    cohort = read(historical_path)
    original_protocol = read(historical_path.parent / cohort['source_protocol']['path'])
    historical_products = {str(c['generation_seed']): original_protocol['spec']['frozen_starts']['entries'][str(c['generation_seed'])]['generation']['draw']['product']
        for c in cohort['cases']}
    small = {seed for seed, product in historical_products.items() if product in launch['spec']['strata']['small']}
    check(len(small) == 15 and len(historical_products) - len(small) == 17,
        'source-invalid', 'report', 'historical strata differ from the registered panel')
    continuation_path = Path(state['continuation']['path'])
    check(sha(continuation_path) == state['continuation']['sha256'], 'source-invalid', 'report', 'continuation report changed')
    continuation = read(continuation_path)
    probes = {}
    for ref in continuation['records']:
        record = read(checked(ref, continuation_path.parent))
        probes[(record['seed'], record['arm'], record['update'])] = record['rows']
    results = {}
    for item in launch['order']:
        seed, arm = item['seed'], item['arm']
        name = f'{arm}-seed-{seed}'
        directory = matrix / name
        job = read(directory / 'state.json')
        check(job['status'] == 'complete' and job['update'] == 128, 'source-invalid', 'report', 'arm endpoint missing', name)
        transfer = {r['update']: panel_values(read(checked(r, directory))['rows']) for r in job['transfer']}
        late = [panel_values(read(checked(r, directory))['rows']) for r in job['historical'] if r['update'] in range(100, 129, 4)]
        check(len(late) == 8, 'source-invalid', 'report', 'late historical window coverage differs', name)
        latest = read(checked(job['records'][-1], directory))
        updates = [read(checked(ref, directory)) for ref in job['records']]
        check(len(updates) == 128 and sum(r['actions'] for r in updates) == 32768
            and sum(r['completed_episodes'] for r in updates) == 512,
            'source-invalid', 'report', 'registered training action/episode budget differs', name)
        draws_origin, draws_stratum, imitation_groups = Counter(), Counter(), {}
        for update in updates:
            success = update['ppo']['success']
            draws_origin.update(success['draws_by_origin'])
            draws_stratum.update(success['draws_by_stratum'])
            for group in success['groups']:
                key = (group['origin'], group['stratum'], group['stage'])
                total = imitation_groups.setdefault(key, dict(origin=key[0], stratum=key[1], stage=key[2],
                    labels=0, positive=0, zero=0, weight_sum=0., weighted_nll_sum=0.))
                for field in ('labels', 'positive', 'zero', 'weight_sum', 'weighted_nll_sum'):
                    total[field] += group[field]
        anchors = {u: read(directory / f'anchors/{u:04d}.json')['rows'] for u in (0, 128)}
        stage = {}
        for key in ('pose', 'repair', 'commit'):
            initial = [r for r in probes[(seed, 'C', 0)] if r['stage'] == key]
            final = [r for r in probes[(seed, arm, 128)] if r['stage'] == key]
            check(len(initial) == len(final) == 40, 'source-invalid', 'report', 'stage continuation coverage differs', name)
            stage[key] = dict(initial_target_rate=mean(r['demonstrated_target_attained'] for r in initial),
                target_rate=mean(r['demonstrated_target_attained'] for r in final),
                improvement_rate=mean(r['improved'] for r in final), valid_commit_rate=mean(r['valid_commit'] for r in final),
                abandon_rate=mean(r['abandoned'] for r in final), wall_guard_rate=mean(r['guard'] == 'branch-wall' for r in final),
                initial_probability=mean(r['probability'] for r in anchors[0] if r['stage'] == key),
                probability=mean(r['probability'] for r in anchors[128] if r['stage'] == key))
        results[name] = dict(seed=seed, arm=arm, seconds=job['seconds'], own_entries=latest['buffer']['own_entries'],
            own_admissions=latest['buffer']['own_admissions'], replay=latest['buffer'],
            learning_totals=dict(draws_by_origin=dict(draws_origin), draws_by_stratum=dict(draws_stratum),
                imitation_groups=list(imitation_groups.values()),
                actor_epoch_counts=dict(Counter(r['ppo']['actor_epochs'] for r in updates)),
                training_area_reward=sum(r['area_reward'] for r in updates),
                mean_advantage_std=mean(r['ppo']['raw_advantage_std'] for r in updates)),
            transfer_by_update=transfer, strict_final=mean(transfer[128][key] for key in eligible) if eligible else None,
            strict_initial=mean(transfer[0][key] for key in eligible) if eligible else None,
            all16_final=mean(transfer[128].values()), all16_initial=mean(transfer[0].values()),
            late_small=mean(row[key] for row in late for key in small),
            late_large=mean(row[key] for row in late for key in row if key not in small),
            late_all=mean(v for row in late for v in row.values()),
            improved_eligible_factories=sum(transfer[128][key] > 0 for key in eligible),
            continuation=stage, tracking_url=job['wandb_url'])
    contrasts, gates = {}, {}
    for seed in launch['spec']['seeds']:
        selected = {arm['name']: results[f'{arm["name"]}-seed-{seed}'] for arm in launch['spec']['arms']}
        contrasts[str(seed)] = {}
        for treatment, control in [('U05', 'C'), ('B05', 'U05'), ('B50', 'U50'), ('U50', 'U05'), ('B50', 'B05')]:
            contrasts[str(seed)][f'{treatment}-{control}'] = {metric: selected[treatment][metric] - selected[control][metric]
                for metric in ('strict_final', 'all16_final', 'late_small', 'late_large', 'late_all') if selected[treatment][metric] is not None}
        contrasts[str(seed)]['interaction'] = {metric: selected['B50'][metric] - selected['U50'][metric] - selected['B05'][metric] + selected['U05'][metric]
            for metric in ('strict_final', 'all16_final', 'late_small', 'late_large', 'late_all') if selected['B50'][metric] is not None}
    control_small = mean(results[f'C-seed-{s}']['late_small'] for s in launch['spec']['seeds'])
    tolerance = max(.2, .1 * control_small)
    for arm in [a['name'] for a in launch['spec']['arms'] if a['name'] != 'C']:
        differences = []
        for seed in launch['spec']['seeds']:
            target, control = results[f'{arm}-seed-{seed}'], results[f'C-seed-{seed}']
            differences.append(dict(seed=seed,
                strict_delta=None if target['strict_final'] is None else target['strict_final'] - control['strict_final'],
                own_initial_delta=None if target['strict_final'] is None else target['strict_final'] - target['strict_initial'],
                pose_target_rate_delta=target['continuation']['pose']['target_rate'] - control['continuation']['pose']['target_rate'],
                small_delta=target['late_small'] - control['late_small'], improved_factories=target['improved_eligible_factories']))
        values = [r['strict_delta'] for r in differences]
        transfer_pass = len(eligible) >= launch['spec']['transfer_panel']['minimum_eligible'] and \
            sum(v is not None and v > 0 for v in values) >= 2 and all(v is not None and v >= 0 for v in values) and \
            sum(r['improved_factories'] >= 2 for r in differences) >= 2
        gates[arm] = dict(per_seed=differences, known_state_progress=sum(r['pose_target_rate_delta'] >= .25 for r in differences) >= 2,
            transfer_candidate=transfer_pass, transfer_inconclusive=len(eligible) < 12,
            small_preservation=mean(r['small_delta'] for r in differences) >= -tolerance,
            small_tolerance_pp=tolerance, pooled_small_delta=mean(r['small_delta'] for r in differences))
    result = dict(schema='blueprint-replay-balance-report-v1', status='complete', launch_sha256=state['launch_sha256'],
        matrix_seconds=state['seconds'], eligibility=eligibility, eligible_cases=len(eligible), training_products=dict(products),
        arms=results, contrasts=contrasts, gates=gates, interpretation='Three learner seeds share one actual input sequence; screening evidence only.')
    write(output, result)
    return result
