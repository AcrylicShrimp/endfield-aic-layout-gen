"""Compare matched initial factories and retained traces in two bounded PPO runs."""
import argparse
from collections import Counter, defaultdict
import gzip
import hashlib
import json
import math
from pathlib import Path
import statistics


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def mean(values):
    return statistics.mean(values) if values else None


def quantile(values, fraction):
    values = sorted(values)
    return values[min(len(values) - 1, int((len(values) - 1) * fraction))] if values else None


def records(root, batches):
    state = read(root / 'state.json')
    assert len(state['jobs']) == 1, 'comparison expects one initialization seed per run'
    rows = {}
    for reference in state['jobs'][0]['records']:
        phase = reference['phase']
        if phase['batch'] > batches:
            continue
        path = root / reference['path']
        assert sha(path) == reference['sha256'], f'record hash drift: {path}'
        key = phase['kind'], phase['batch']
        assert key not in rows, f'duplicate phase: {key}'
        rows[key] = read(path)
    return state, rows


def completed(record):
    return {r['identity']['generation_seed']: r for r in record['results'] if r['status'] == 'complete'}


def reduction(result):
    row = result['row']
    return 100 * (1 - row['best_metrics']['score'][0] / row['initial_metrics']['score'][0])


def traces(record):
    found = {}
    for seed, result in completed(record).items():
        path = Path(result['row']['directory']) / 'episode.json.gz'
        assert path.is_file(), f'retained evaluation trace missing: {path}'
        episode = json.loads(gzip.decompress(path.read_bytes()))
        assert episode['selected'] == len(episode['trace']) == result['row']['selected']
        assert episode['best_metrics'] == result['row']['best_metrics']
        found[seed] = episode
    return found


def action_category(action):
    if action['kind'] == 'pose':
        return 'pose_group' if len(action['poses']) > 1 else 'pose_single'
    return action['kind']


def future_credit(episodes):
    by_position = defaultdict(list)
    cases = []
    for episode in episodes.values():
        trace = episode['trace']
        remaining = 0.
        rows = []
        for position in range(len(trace) - 1, -1, -1):
            step = trace[position]
            remaining += step['reward']
            assert math.isclose(remaining, step['reward_to_go'], rel_tol=1e-10, abs_tol=1e-10)
            gain = 100 * remaining
            rows.append((position, action_category(step['action']), gain))
            by_position[position].append(gain)
        cases.append(rows)
    position_means = {position: mean(values) for position, values in by_position.items()}
    values, centered = defaultdict(list), defaultdict(list)
    for rows in cases:
        for position, kind, gain in rows:
            values[kind].append(gain)
            centered[kind].append(gain - position_means[position])
    return dict(categories={kind: dict(selected=len(v), summed_future_gain_pp=sum(v),
                                      mean_future_gain_pp=mean(v), mean_position_centered_pp=mean(centered[kind]))
                            for kind, v in values.items()},
                mean_by_position=[position_means[p] for p in sorted(position_means)])


def trace_stats(episodes):
    steps = [step for episode in episodes.values() for step in episode['trace']]
    categories = defaultdict(list)
    regions = Counter()
    terminations = Counter()
    for step in steps:
        action = step['action']
        categories[action['kind']].append(step)
        if action['kind'] == 'pose':
            categories['pose_group' if len(action['poses']) > 1 else 'pose_single'].append(step)
            if abs(action['x'] - action['source_x']) + abs(action['y'] - action['source_y']) >= 8:
                categories['pose_displacement_ge8'].append(step)
        region = step.get('region')
        if region:
            attempts = region['attempts']
            regions['reported'] += 1
            terminations[region['termination']] += 1
            assert region['route_expansions'] == step['route_expansions'] <= region['work_budget']
            assert region['route_expansions'] == sum(a['route_expansions'] for a in attempts)
            retried = len(attempts) > 1
            expanded = any(a['extra_chains'] for a in attempts)
            ports = any(a['port_assignments'] != attempts[0]['port_assignments'] for a in attempts)
            valid = step.get('acceptance', {}).get('candidate_validated', False)
            for label, present in [('retried', retried), ('expanded', expanded), ('port_retry', ports)]:
                if present:
                    regions[label] += 1
                    regions[label + '_validated'] += int(valid)
                    regions[label + '_area_improved'] += int(step['reward_numerator'] > 0)
    def summarize(rows):
        return dict(selected=len(rows), statuses=dict(Counter(s['status'] for s in rows)),
                    validated=sum(s.get('acceptance', {}).get('candidate_validated', False) for s in rows),
                    area_improved=sum(s['reward_numerator'] > 0 for s in rows),
                    reward_sum=sum(s['reward'] for s in rows),
                    mean_executor_ms=1000 * mean([s['executor_seconds'] for s in rows]),
                    p95_executor_ms=1000 * quantile([s['executor_seconds'] for s in rows], .95),
                    route_expansions=sum(s['route_expansions'] for s in rows))
    return dict(all=summarize(steps), categories={k: summarize(v) for k, v in categories.items()},
                regions=dict(regions), region_terminations=dict(terminations))


def compare(old_root, new_root, batches, output):
    old_state, old = records(old_root, batches)
    new_state, new = records(new_root, batches)
    assert new_state['status'] == 'complete', 'new run must finish normally'
    assert read(new_root / 'protocol.json')['config']['training']['batches'] == batches
    assert sorted(k[1] for k in new if k[0] == 'training') == list(range(1, batches + 1))
    assert set(new) <= set(old)
    matched = set()
    for phase, record in new.items():
        before = {r['identity']['generation_seed']: r for r in old[phase]['results']}
        after = {r['identity']['generation_seed']: r for r in record['results']}
        assert set(before) == set(after), f'case set differs: {phase}'
        for seed, result in after.items():
            original = before[seed]
            assert original['generation']['status'] == result['generation']['status']
            for key in ('request_sha256', 'initial_sha256', 'geometry_sha256', 'initial_metrics'):
                assert original['generation'].get(key) == result['generation'].get(key), (phase, seed, key)
            matched.add(seed)
    initial_identities = [{(r['policy_sha256'], r['critic_sha256']) for r in run['evaluation', 0]['results']}
                          for run in (old, new)]
    assert initial_identities[0] == initial_identities[1] and len(initial_identities[0]) == 1
    eval_batches = sorted(k[1] for k in new if k[0] == 'evaluation')
    cohort = sorted(set.intersection(*(set(completed(run['evaluation', b])) for run in (old, new) for b in eval_batches)))
    points, trajectories, stats, contributions, credits = [], {}, {}, [], []
    initial = [completed(run['evaluation', 0]) for run in (old, new)]
    for batch in eval_batches:
        contribution = dict(batch=batch)
        credit = dict(batch=batch)
        paired = [completed(run['evaluation', batch]) for run in (old, new)]
        values = [[reduction(rows[seed]) for seed in cohort] for rows in paired]
        differences = [b - a for a, b in zip(*values)]
        points.append(dict(batch=batch, paired_cases=len(cohort), baseline_mean=mean(values[0]), region_mean=mean(values[1]),
                           region_minus_baseline_pp=mean(differences),
                           baseline_learning_delta_pp=mean([reduction(paired[0][s]) - reduction(initial[0][s]) for s in cohort]),
                           region_learning_delta_pp=mean([reduction(paired[1][s]) - reduction(initial[1][s]) for s in cohort]),
                           wins=sum(d > 1e-9 for d in differences), ties=sum(abs(d) <= 1e-9 for d in differences),
                           losses=sum(d < -1e-9 for d in differences),
                           pairs=[dict(seed=s, product=paired[0][s]['generation']['draw']['product'], baseline=a, region=b)
                                  for s, a, b in zip(cohort, *values)]))
        for label, run in [('baseline', old), ('region', new)]:
            episodes = {seed: episode for seed, episode in traces(run['evaluation', batch]).items() if seed in cohort}
            stats[f'{label}-{batch}'] = trace_stats(episodes)
            credit[label] = future_credit(episodes)
            categories = stats[f'{label}-{batch}']['categories']
            # Split poses into disjoint single/group categories. The distant
            # statistic overlaps both, so it cannot enter an additive outcome.
            contribution[label] = {kind: dict(selected=row['selected'], area_improved=row['area_improved'],
                                              area_reduction_pp=100 * row['reward_sum'] / len(cohort))
                                   for kind, row in categories.items() if kind not in ('pose', 'pose_displacement_ge8')}
            total = sum(row['area_reduction_pp'] for row in contribution[label].values())
            assert math.isclose(total, points[-1][label + '_mean'], rel_tol=1e-10, abs_tol=1e-10), 'action gains must sum to outcome'
            curves = []
            for seed in cohort:
                episode = episodes[seed]
                area = episode['initial_metrics']['score'][0]
                curves.append([0.] + [100 * (1 - s['best_score'][0] / area) for s in episode['trace']])
            assert len({len(c) for c in curves}) == 1, 'trajectory horizons differ'
            trajectories[f'{label}-{batch}'] = [mean(list(v)) for v in zip(*curves)]
        contributions.append(contribution)
        credits.append(credit)
    costs = {}
    for label, run in [('baseline', old), ('region', new)]:
        training = [run['training', b] for b in range(1, batches + 1)]
        costs[label] = dict(selected=sum(r['quality']['selected'] for r in training),
                            phase_seconds=sum(r['phase_seconds'] for r in training),
                            generation_worker_seconds=sum(r['quality']['total_generation_seconds'] for r in training),
                            episode_worker_seconds=sum(r['quality']['total_episode_seconds'] for r in training),
                            update_seconds=sum(r['update_seconds'] for r in training),
                            mean_training_area_reduction=mean([r['quality']['mean_area_reduction_percent'] for r in training]))
    value = dict(schema='ppo-region-paired-comparison-v1', batches=batches, matched_unique_starts=len(matched),
                 sources={label: dict(root=str(root), protocol_sha256=sha(root / 'protocol.json'), state_sha256=sha(root / 'state.json'))
                          for label, root in [('baseline', old_root), ('region', new_root)]},
                 initial_parameters=list(initial_identities[0])[0], cohort=cohort, evaluation=points,
                 trajectories=trajectories, evaluation_trace_stats=stats, training_costs=costs,
                 action_contributions=contributions,
                 future_action_credit=credits,
                 definitions=dict(displacement='Pose envelope-origin Manhattan displacement >= 8 cells; group and single poses overlap this category.',
                                  quality='Mean per-factory percent best-area reduction from the same initial factory.',
                                  action_contribution='Sum of immediate best-area gain / initial area, averaged across paired factories. Disjoint action kinds; single/group poses separated. Exact additive accounting, not causal or indirect credit.',
                                  future_action_credit='Every best-area gain is credited to every action up to and including the action producing it: G[t] = sum(reward[u], u >= t), gamma=1. Totals count each outcome repeatedly. Means condition on selected action kind. Position-centered values subtract the same executor/checkpoint action-index mean across factories; state confounding remains.',
                                  caveat='Single initialization seed. Frozen input admission excludes old construction/setup cost.'))
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    print(json.dumps({k: value[k] for k in ('batches', 'matched_unique_starts', 'training_costs')}))
    for point in points:
        print(json.dumps({k: v for k, v in point.items() if k != 'pairs'}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--region', type=Path, required=True)
    parser.add_argument('--batches', type=int, default=32)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    compare(args.baseline.resolve(), args.region.resolve(), args.batches, args.output.resolve())
