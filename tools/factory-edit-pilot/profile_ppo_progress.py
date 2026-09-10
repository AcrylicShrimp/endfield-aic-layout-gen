"""Read a bounded committed PPO snapshot and audit action contributions without GPU work."""
import argparse
from collections import Counter
import json
import math
from pathlib import Path
import statistics

from audit_ppo_action_contributions import FAILURES, add_episode, bucket, check, finish, read, sha


def summarize(results):
    rows = [r['row'] for r in results if r['status'] == 'complete']
    check(bool(rows), 'at least one complete episode required')
    selected = sum(r['selected'] for r in rows)
    statuses, kinds, causes, area = (Counter() for _ in range(4))
    for row in rows:
        statuses.update(row['statuses'])
        kinds.update(row['kinds'])
        causes.update(row['routing_failure_codes'])
        area.update({k: 100 * v for k, v in row['area_reward_by_kind'].items()})
    failed = sum(statuses[k] for k in FAILURES)
    check(failed == sum(r['failed_actions'] for r in rows), 'failure counters differ')
    check(sum(kinds.values()) == sum(statuses.values()) == selected, 'action counters differ')
    mean_area = 100 * sum(r['area_reward_sum'] for r in rows) / len(rows)
    check(math.isclose(sum(area.values()) / len(rows), mean_area, abs_tol=1e-10), 'row area attribution differs')
    budget = sum(r['routing_budget_failed_actions'] for r in rows)
    return dict(attempts=len(results), episodes=len(rows), selected=selected, failed=failed,
        failure_percent=100 * failed / selected, budget_failed=budget, budget_failure_percent=100 * budget / selected,
        statuses=dict(statuses), kinds=dict(kinds), failure_codes=dict(causes),
        mean_area_reduction_percent=mean_area, contribution_pp={k: v / len(rows) for k, v in area.items()},
        mean_first_eight_pp=100 * sum(r['first_eight_area_reward_sum'] for r in rows) / len(rows),
        mean_later_pp=100 * sum(r['later_area_reward_sum'] for r in rows) / len(rows),
        mean_failure_cost_pp=100 * sum(r['failure_cost_sum'] for r in rows) / len(rows),
        accepted_unchanged=sum(r['accepted_physical_unchanged_actions'] for r in rows),
        area_improving=sum(r['area_improving_actions'] for r in rows),
        first_attempt_budget_failures=sum(r['routing_first_attempt_budget_failures'] for r in rows))


def training_window(records):
    results = [r for record in records for r in record['results']]
    epochs = [len(r['ppo']['epochs']) for r in records]
    depths = sorted({r['generation']['start_curriculum']['depth'] for r in results})
    return dict(batches=[r['phase']['batch'] for r in records], **summarize(results),
        epochs=epochs, mean_epochs=statistics.mean(epochs),
        kl_stopped=sum(r['ppo']['kl_stopped'] for r in records),
        mean_final_kl=statistics.mean(r['ppo']['epochs'][-1]['approximate_kl'] for r in records),
        mean_entropy=statistics.mean(r['ppo']['epochs'][-1]['entropy'] for r in records),
        median_update_seconds=statistics.median(r['update_seconds'] for r in records),
        median_phase_seconds=statistics.median(r['phase_seconds'] for r in records),
        depths={str(d): summarize([r for r in results if r['generation']['start_curriculum']['depth'] == d]) for d in depths})


def profile(args):
    root, output = args.run.resolve(), args.output.resolve()
    check(not output.exists(), f'fresh output required: {output}')
    check(args.through > 0 and args.training_window > 0 and args.evaluation_window > 0, 'positive snapshot/window sizes required')
    inputs = []
    protocol = read(root / 'protocol.json', inputs)
    state_bytes = (root / 'state.json').read_bytes()
    state = json.loads(state_bytes)
    inputs.append(dict(path=str(output / 'state-snapshot.json'), source_path=str(root / 'state.json'), sha256=sha(state_bytes)))
    check(len(state['jobs']) == 1, 'one job required')
    check(state['protocol_sha256'] == inputs[0]['sha256'], 'protocol/state identity mismatch')
    job = state['jobs'][0]
    check(job['completed_batches'] >= args.through, 'requested batch is not committed')
    records = []
    for ref in job['records']:
        if ref['phase']['batch'] <= args.through:
            record = read(root / ref['path'], inputs)
            check(inputs[-1]['sha256'] == ref['sha256'], f'record hash mismatch: {ref["path"]}')
            records.append(record)
    train = [r for r in records if r['phase']['kind'] == 'training']
    evaluations = [r for r in records if r['phase']['kind'] == 'evaluation']
    check([r['phase']['batch'] for r in train] == list(range(1, args.through + 1)) and evaluations,
          'complete prefix and evaluation records required')
    recent_evaluations = evaluations[-args.evaluation_window:]
    recent_batches = {r['phase']['batch'] for r in recent_evaluations}
    recent = bucket()
    cohort = None
    series = []
    for record in evaluations:
        signature = sorted((r['identity']['generation_seed'], r['generation']['initial_sha256']) for r in record['results'])
        if cohort is None:
            cohort = signature
        check(signature == cohort, 'evaluation cohort changed')
        actions = bucket()
        for result in record['results']:
            check(result['status'] == 'complete', 'incomplete evaluation; refuse partial action attribution')
            row = result['row']
            episode = read(Path(row['directory']) / 'episode.json.gz', inputs)
            add_episode(actions, row, episode['trace'])
            if record['phase']['batch'] in recent_batches:
                add_episode(recent, row, episode['trace'])
        actions = finish(actions)
        check(math.isclose(actions['mean_area_reduction_percent'], record['quality']['mean_area_reduction_percent'], abs_tol=1e-10),
              'evaluation trace/quality mismatch')
        series.append(dict(batch=record['phase']['batch'], **summarize(record['results']), action_audit=actions))
    recent_summary = summarize([r for record in recent_evaluations for r in record['results']])
    report = dict(schema='ppo-progress-action-snapshot-v1', run=str(root), through_training_batch=args.through,
        source_sha256=sha(Path(__file__).read_bytes()), protocol_sha256=state['protocol_sha256'],
        training_actions=sum(r['ppo']['selected'] for r in train),
        training_early=training_window(train[:args.training_window]), training_recent=training_window(train[-args.training_window:]),
        evaluation_recent=dict(batches=[r['phase']['batch'] for r in recent_evaluations], **recent_summary, action_audit=finish(recent)),
        evaluations=series,
        limitations=['Repeated fixed-cohort evaluations are not independent factory samples.',
                    'Action contributions are realized best-area reductions, not causal ablations.',
                    'Local and region failure-code tiers overlap and must not be added together.',
                    'Training per-kind rejection rates are unavailable without all training traces.'])
    output.mkdir(parents=True)
    (output / 'state-snapshot.json').write_bytes(state_bytes)
    manifest = (json.dumps(inputs, indent=2) + '\n').encode()
    (output / 'inputs.json').write_bytes(manifest)
    report['input_manifest_sha256'] = sha(manifest)
    (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    if args.plot:
        plot(report, output / 'progress.png')
    print(json.dumps(dict(output=str(output), through=args.through, training_actions=report['training_actions'],
                         recent_evaluation=recent_summary, report_sha256=sha((output / 'report.json').read_bytes()))))


def plot(report, output):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    rows = report['evaluations']
    x = [r['batch'] for r in rows]
    fig, axes = plt.subplots(2, 2, figsize=(12, 8), layout='constrained')
    fig.suptitle('Fresh padded PPO: fixed 32-factory evaluation cohort', fontsize=16, fontweight='bold')
    for ax in axes.flat:
        ax.spines[['top', 'right']].set_visible(False)
        ax.grid(axis='y', alpha=.2)
        ax.set_xlabel('Completed training batches')
    kinds, colors = ['contract', 'pose'], ['#397eaf', '#eda441']
    axes[0, 0].stackplot(x, *[[r['contribution_pp'].get(k, 0) for r in rows] for k in kinds],
                        labels=kinds, colors=colors, alpha=.9)
    axes[0, 0].plot(x, [r['mean_area_reduction_percent'] for r in rows], color='#222222', linewidth=1.5, label='Total')
    axes[0, 0].set(title='Direct area contribution', ylabel='Percentage points of initial area')
    axes[0, 0].legend(frameon=False, loc='lower right')
    for key, label, color in [('failure_percent', 'All rejected', '#c35656'), ('budget_failure_percent', 'Routing budget', '#7d6baa')]:
        axes[0, 1].plot(x, [r[key] for r in rows], label=label, color=color)
    axes[0, 1].plot(x, [r['action_audit']['kinds'].get('pose', {}).get('failure_percent', 0) for r in rows],
                    color='#eda441', label='Pose rejected / pose selected')
    axes[0, 1].set(title='Execution rejection rates', ylabel='Percent')
    axes[0, 1].legend(frameon=False)
    for key, label, color in [('mean_first_eight_pp', 'Actions 1-8', '#177e89'), ('mean_later_pp', 'Actions 9-32', '#b872b4')]:
        axes[1, 0].plot(x, [r[key] for r in rows], label=label, color=color)
    axes[1, 0].set(title='When area improvements occur', ylabel='Percentage points of initial area')
    axes[1, 0].legend(frameon=False)
    mix = ['contract', 'pose', 'restore_best', 'port', 'reroute']
    values = [[100 * r['kinds'].get(k, 0) / r['selected'] for r in rows] for k in mix]
    values.append([100 - sum(group[i] for group in values) for i in range(len(rows))])
    axes[1, 1].stackplot(x, *values, labels=mix + ['Other'], colors=colors + ['#70a58a', '#b872b4', '#7d8795', '#c9c9c9'])
    axes[1, 1].set(title='Selected action mix', ylabel='Percent of all selections', ylim=(0, 100))
    axes[1, 1].legend(frameon=False, loc='upper center', ncol=3, fontsize=8)
    for ax in axes.flat:
        ax.set_ylim(bottom=0)
    fig.savefig(output, dpi=160, bbox_inches='tight', pad_inches=.15)
    plt.close(fig)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--through', type=int, required=True)
    parser.add_argument('--training-window', type=int, default=20)
    parser.add_argument('--evaluation-window', type=int, default=5)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--plot', action='store_true', help='Create a static figure; requires matplotlib in the diagnostic environment')
    profile(parser.parse_args())
