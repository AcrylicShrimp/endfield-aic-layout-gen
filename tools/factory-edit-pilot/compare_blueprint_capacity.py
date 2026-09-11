"""Analyze a completed registered capacity matrix without selecting peak checkpoints."""
import argparse
from collections import Counter
from copy import deepcopy
from pathlib import Path
from statistics import mean

from blueprint_duration_metrics import evaluation_metrics
from blueprint_ppo_store import checked, read, reference, sha, write
from blueprint_ppo_update import require


def common_config(config):
    result = deepcopy(config)
    for key in ('purpose', 'seeds', 'experiment'):
        result.pop(key)
    for key in ('width', 'message_rounds'):
        result['ppo'].pop(key)
    return result


def compare_rows(candidate, baseline):
    identity = lambda r: (r['generation_seed'], r['sample'])
    left, right = ({identity(r): r for r in rows} for rows in (candidate, baseline))
    require(len(left) == len(candidate) and left.keys() == right.keys(), 'paired capacity cohort')
    counts = Counter(win=0, tie=0, loss=0)
    for key, row in left.items():
        other = right[key]
        require(all(row[k] == other[k] for k in
                    ('start_score', 'request_sha256', 'initial_sha256', 'action_seed')), 'paired capacity reset')
        difference = row['total_area_reduction_percent'] - other['total_area_reduction_percent']
        counts['win' if difference > 1e-10 else 'loss' if difference < -1e-10 else 'tie'] += 1
    return dict(counts)


def analyze(output):
    matrix, state = read(output / 'matrix.json'), read(output / 'state.json')
    require(state['status'] == 'complete' and state['matrix_sha256'] == sha(output / 'matrix.json'),
            'completed source-bound matrix required')
    spec, arms, curves, finals, shared = matrix['spec'], [], [], {}, None
    for arm in matrix['arms']:
        directory, seed = output / arm['directory'], arm['seed']
        protocol = read(checked(arm['protocol'], output))
        config = protocol['config']
        factors = arm['model']
        require(config['seeds'] == [seed] and config['training']['updates'] == spec['updates'], 'matched seed/budget')
        require(all(config['ppo'][k] == factors[k] for k in ('width', 'message_rounds')), 'registered architecture')
        require(config['experiment']['matrix_sha256'] == matrix['matrix_sha256'], 'registered matrix identity')
        normalized = dict(config=common_config(config), native_sha256=protocol['native_sha256'],
                          runtime=protocol['runtime'], external_sources=protocol['external_sources'])
        if shared is None:
            shared = normalized
        require(normalized == shared, 'capacity comparison changed a non-factor input')
        parent = state['arms'][arm['name']]
        require(parent['status'] == 'complete', 'completed matrix arm')
        completion = read(checked(parent['completion'], output))
        child = read(checked(parent['state'], output))
        require(completion['status'] == child['status'] == 'complete', 'completed child')
        checked(completion['state'], directory)
        audit = read(checked(completion['tail_audit'], directory))
        require(audit['status'] == 'passed', 'passing final audit')
        observer = read(checked(parent['observer'], output))
        require(observer['status'] == 'finished', 'finished tracking')
        job = child['jobs'][str(seed)]
        require(job['update'] == spec['updates'] and len(job['records']) == spec['updates'], 'full training budget')
        require(job['initialization']['evaluated_before_training'], 'untrained evaluation present')
        reports = [read(checked(ref, directory)) for ref in job['evaluations']]
        interval = config['evaluation']['interval_batches']
        require([r['update'] for r in reports] == list(range(0, spec['updates'] + 1, interval)), 'evaluation cadence')
        require(all(len(r['rows']) == config['evaluation']['expected_cases'] for r in reports), 'complete evaluation cohort')
        initial = reports[0]['rows']
        points = []
        for report, ref in zip(reports, job['evaluations']):
            rows = report['rows']
            measured = evaluation_metrics(rows, initial)
            require(all(r['action_seed'] == b['action_seed'] for r, b in zip(rows, initial)), 'fixed evaluation sampling seeds')
            point = dict(arm=arm['name'], variant=factors['name'], seed=seed, update=report['update'],
                         actions=report['update'] * config['training']['streams'] * config['training']['chunk_actions'],
                         seconds=ref['seconds'],
                         area_reduction_percent=measured['evaluation/all_admitted/by_batches/area_reduction_percent'],
                         delta_from_initial_pp=measured['evaluation/all_admitted/by_batches/delta_from_initial_pp'])
            points.append(point)
        curves.extend(points)
        records = [read(checked(ref, directory)) for ref in job['records']]
        final, rows = points[-1], [row for report in reports for row in report['rows']]
        window = [p for p in points if p['update'] > spec['updates'] - spec['last_window_batches']]
        guards = Counter(r['guard'] for r in rows)
        require(all(r['policy_actions'] <= config['evaluation']['max_actions'] for r in rows), 'evaluation action cap')
        actions = sum(r['actions'] for r in records)
        training_window = records[-spec['last_window_batches']:]
        completed_window = sum(r['quality'].get('accepted', 0) for r in training_window)
        require(completed_window > 0, 'completed episodes in the registered training window')
        final_counts = sum((Counter(r['counts']) for r in reports[-1]['rows']), Counter())
        arm_result = dict(name=arm['name'], variant=factors['name'], seed=seed, cpu=parent['cpu'],
            width=factors['width'], message_rounds=factors['message_rounds'],
            parameters=config['experiment']['parameters'], training_actions=actions,
            completed_episodes=sum(r['quality'].get('accepted', 0) for r in records),
            initial_area_reduction_percent=points[0]['area_reduction_percent'],
            final_area_reduction_percent=final['area_reduction_percent'],
            delta_from_initial_pp=final['delta_from_initial_pp'],
            last_window_mean_percent=mean(p['area_reduction_percent'] for p in window),
            last_window_training_mean_percent=sum(r['quality'].get('accepted', 0) *
                r['quality'].get('mean_area_reduction_percent', 0.) for r in training_window) / completed_window,
            last_window_evaluations=len(window), active_seconds=child['seconds'],
            collection_seconds=sum(r['collection_seconds'] for r in records),
            optimizer_seconds=sum(r['ppo']['seconds'] for r in records),
            mean_evaluation_inference_ms=1000 * mean(r['inference_seconds'] for r in rows),
            evaluation_inference_seconds=sum(r['inference_seconds'] for r in rows),
            mean_evaluation_elapsed_fraction=mean(r['elapsed_ms'] / config['evaluation']['elapsed_ms'] for r in rows),
            evaluation_cases=len(rows), evaluation_guards=dict(guards),
            full_action_cases=sum(r['policy_actions'] == config['evaluation']['max_actions'] for r in rows),
            final_improved_cases=sum(r['total_area_reduction_percent'] > 0 for r in reports[-1]['rows']),
            final_evaluation_counts=dict(final_counts),
            positive_commits=records[-1]['buffer']['positive_commits'],
            buffer_entries=records[-1]['buffer']['entries'],
            positive_replay_labels=sum(r['ppo']['success']['positive_labels'] for r in records),
            url=observer['url'], protocol=arm['protocol'], completion=parent['completion'], audit=parent['audit'])
        arms.append(arm_result)
        finals[(factors['name'], seed)] = reports[-1]['rows']
    for arm in arms:
        arm['paired_final_vs_baseline'] = compare_rows(finals[(arm['variant'], arm['seed'])],
                                                      finals[('w40-r2', arm['seed'])])
    aggregates = []
    for model in spec['models']:
        selected = [a for a in arms if a['variant'] == model['name']]
        require(sorted(a['seed'] for a in selected) == sorted(spec['seeds']), 'all registered model seeds')
        row = dict(variant=model['name'], parameters=selected[0]['parameters'])
        for key in ('initial_area_reduction_percent', 'final_area_reduction_percent', 'delta_from_initial_pp',
                    'last_window_mean_percent', 'last_window_training_mean_percent',
                    'active_seconds', 'collection_seconds', 'optimizer_seconds',
                    'mean_evaluation_inference_ms'):
            row[key] = mean(a[key] for a in selected)
        row['paired_final_vs_baseline'] = {k: sum(a['paired_final_vs_baseline'][k] for a in selected)
                                          for k in ('win', 'tie', 'loss')}
        aggregates.append(row)
    return dict(schema='blueprint-capacity-analysis-v1', matrix=reference(output / 'matrix.json', output),
                state=reference(output / 'state.json', output), spec=spec, arms=arms, aggregates=aggregates, curves=curves,
                limitations=['Fixed-budget early capacity screen; convergence is untested.',
                             'Two training seeds; one five-facility training fixture in two starts.',
                             'Native elapsed guards and actual CPU allocation remain measured factors.',
                             'Elapsed time remains a policy feature, so runtime differences can also change policy inputs.'])


def plot(result, destination):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    figure, axes = plt.subplots(1, 2, figsize=(13, 4.5), constrained_layout=True)
    colors = dict(zip((m['name'] for m in result['spec']['models']), ('#2563eb', '#dc2626', '#059669', '#9333ea')))
    for axis, x, label in zip(axes, ('update', 'seconds'), ('Optimizer batches', 'Active elapsed seconds')):
        for arm in result['arms']:
            points = [p for p in result['curves'] if p['arm'] == arm['name']]
            axis.plot([p[x] for p in points], [p['area_reduction_percent'] for p in points],
                      color=colors[arm['variant']], linestyle='-' if arm['seed'] == 17 else '--',
                      linewidth=1.2, alpha=.8, label=arm['name'])
        axis.set(xlabel=label, ylabel='Mean validated area reduction (%)', ylim=(0, None))
        axis.grid(alpha=.2)
    axes[1].legend(fontsize=8)
    figure.suptitle('Blueprint model capacity: fixed 32-factory evaluation, 64 actions per case')
    figure.savefig(destination, dpi=180)
    plt.close(figure)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--plot', type=Path)
    args = parser.parse_args()
    result = analyze(args.matrix.resolve())
    result['analysis_source_sha256'] = sha(Path(__file__))
    write(args.output, result)
    if args.plot:
        plot(result, args.plot)
    for row in result['aggregates']:
        print(row)


if __name__ == '__main__':
    main()
