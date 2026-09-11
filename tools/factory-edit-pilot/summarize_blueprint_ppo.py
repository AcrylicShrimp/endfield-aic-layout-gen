"""Summarize and plot an audited persistent-blueprint learning screen."""
import argparse
from collections import Counter
from pathlib import Path
import statistics

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from blueprint_ppo_store import checked, load, read, reference, sha, write
from blueprint_ppo_update import require
from plot_blueprint_edit import draw


def summarize(run):
    protocol, state, audit = [read(run / name) for name in ('protocol.json', 'state.json', 'audit.json')]
    require(audit['status'] == 'passed' and audit['protocol_sha256'] == sha(run / 'protocol.json'), 'passing audit for this protocol')
    config = protocol['config']
    updates, episodes, improvements, repairs, phases = [], [], [], Counter(), {}
    first_improvement = None
    for seed in config['seeds']:
        for ref in state['jobs'][str(seed)]['records']:
            record = read(checked(ref, run))
            updates.append(record)
            chunk = load(checked(record['chunk'], run))
            for sample in chunk['samples']:
                action = sample['input']['actions'][sample['index']]
                phase_name = 'first-quarter' if record['update'] <= config['training']['updates'] / 4 else 'last-quarter' if record['update'] > 3 * config['training']['updates'] / 4 else 'middle-half'
                phase = phases.setdefault(f'{seed}/{phase_name}', Counter())
                phase['actions'] += 1
                phase[f'kind/{action["kind"]}'] += 1
                phase['entropy_sum'] += sample['entropy']
                phase['area_reward'] += sample['reward']
                if action['kind'] in ('repair', 'repair_all'):
                    before = sum(e['status'] == 'pending' for e in sample['observation']['edges'])
                    repaired = before - sample['after_pending']
                    repairs['attempts'] += 1
                    repairs['actions_reducing_pending'] += repaired > 0
                    repairs['original_connections_repaired'] += max(0, repaired)
                    repairs['all_pending_resolved_actions'] += before > 0 and sample['after_pending'] == 0
                if sample['outcome']['status'] == 'improved':
                    event = dict(seed=seed, update=record['update'], stream=sample['stream'], episode=sample['episode'],
                        start=sample['start'], action=sample['policy_action'], reward=sample['reward'],
                        score=sample['outcome']['best_metrics']['score'], chunk=record['chunk'],
                        observation_before=sample['observation']['best_metrics']['score'])
                    improvements.append(event)
                    if first_improvement is None or (first_improvement[1]['reward'] == 0 and event['reward'] > 0):
                        first_improvement = (sample, event)
            for episode in chunk['completed']:
                start, best = episode['start_best'], episode['result']['best_metrics']['score']
                episodes.append(dict(seed=seed, stream=episode['stream'], episode=episode['episode'], start=episode['start'],
                    start_score=start, best_score=best, policy_gain_percent=100 * (start[0] - best[0]) / start[0],
                    score_improved=best < start,
                    counts=episode['counts'], unfinished_draft=episode['result']['unfinished_draft']))
    by_seed = []
    for seed in config['seeds']:
        selected = [r for r in updates if r['seed'] == seed]
        eps = [e for e in episodes if e['seed'] == seed]
        actor_epochs = Counter(r['ppo']['actor_epochs'] for r in selected)
        by_seed.append(dict(seed=seed, updates=len(selected), episodes=len(eps),
            training_reward=sum(r['area_reward'] for r in selected), improved_commits=sum(r['improved_commits'] for r in selected),
            area_improved_commits=sum(e['reward'] > 0 for e in improvements if e['seed'] == seed),
            improved_episodes=sum(e['policy_gain_percent'] > 0 for e in eps), actor_epochs=dict(actor_epochs),
            value_mae_first=selected[0]['ppo']['epochs'][-1]['value_mae'], value_mae_final=selected[-1]['ppo']['epochs'][-1]['value_mae'],
            max_final_epoch_kl=max(r['ppo']['epochs'][-1]['approximate_kl'] for r in selected),
            max_clip_fraction=max(e['outside_clip_fraction'] for r in selected for e in r['ppo']['epochs'])))
    evaluation = audit['evaluation']
    delayed_guards = []
    for row in evaluation:
        if row['elapsed_ms'] <= config['evaluation']['elapsed_ms'] + 10:
            continue
        report = read(run / f'seed-{row["seed"]}/evaluation/{row["update"]:04d}/report.json')
        suffix = f'/{row["start"]}-{row["sample"]}.pt.gz'
        case_ref = next(ref for ref in report['cases'] if ref['path'].endswith(suffix))
        case = load(checked(case_ref, run))
        last = case['trace'][-1]
        require(last['outcome']['status'] == 'rejected' and not last['outcome']['charged']
                and last['best_score'] == last['observation']['best_metrics']['score']
                and last['outcome']['revision'] == last['observation']['revision'],
                'delayed elapsed-guard detection leaves the draft and best unchanged')
        delayed_guards.append(dict(seed=row['seed'], update=row['update'], start=row['start'], sample=row['sample'],
            elapsed_ms=row['elapsed_ms'], last_observation_ms=last['observation']['elapsed_ms'],
            unchanged_late_rejection=True, case=case_ref))
    grouped = []
    for seed in config['seeds']:
        for start in config['evaluation']['starts']:
            for ordinal in config['evaluation']['at']:
                rows = [r for r in evaluation if r['seed'] == seed and r['start'] == start and r['update'] == ordinal]
                grouped.append(dict(seed=seed, start=start, update=ordinal,
                    mean_policy_gain=statistics.mean(r['policy_area_reduction_percent'] for r in rows),
                    best_score=min(r['best_score'] for r in rows), mean_actions=statistics.mean(r['policy_actions'] for r in rows),
                    mean_elapsed_ms=statistics.mean(r['elapsed_ms'] for r in rows),
                    improvements=sum(r['counts'].get('status/improved', 0) for r in rows)))
    paired = []
    final = config['training']['updates']
    for row in evaluation:
        if row['update'] != final:
            continue
        baseline = next(r for r in evaluation if r['update'] == 0 and all(r[k] == row[k] for k in ('seed', 'start', 'sample')))
        paired.append(dict(seed=row['seed'], start=row['start'], sample=row['sample'],
            initial_score=baseline['best_score'], trained_score=row['best_score'],
            delta_area=baseline['best_score'][0] - row['best_score'][0],
            lexicographic_comparison='win' if row['best_score'] < baseline['best_score'] else 'tie' if row['best_score'] == baseline['best_score'] else 'loss',
            delta_policy_gain_percent=row['policy_area_reduction_percent'] - baseline['policy_area_reduction_percent']))
    summary = dict(schema='blueprint-ppo-summary-v1', protocol_sha256=sha(run / 'protocol.json'), audit=reference(run / 'audit.json', run),
        elapsed_seconds=state['seconds'], seed_summary=by_seed, training_repairs=dict(repairs),
        training_improvements=improvements, episodes=episodes, evaluation_groups=grouped, paired_final=paired,
        training_phases={k: dict(v, mean_entropy=v['entropy_sum'] / v['actions']) for k, v in phases.items()},
        area_improvements_after={str(boundary): sum(e['reward'] > 0 and e['action'] > boundary for e in improvements) for boundary in (8, 32, 64)},
        evaluation_guards=dict(Counter(r['guard'] for r in evaluation)),
        evaluation_timing=dict(min_elapsed_ms=min(r['elapsed_ms'] for r in evaluation),
            max_elapsed_ms=max(r['elapsed_ms'] for r in evaluation), delayed_guards=delayed_guards),
        output_bytes=sum(p.stat().st_size for p in run.rglob('*') if p.is_file()))
    write(run / 'summary.json', summary)
    plt.rcParams.update({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False})
    fig, axes = plt.subplots(2, 2, figsize=(12, 8), constrained_layout=True)
    for seed in config['seeds']:
        rows = [r for r in updates if r['seed'] == seed]
        x = [r['update'] for r in rows]
        axes[0, 0].plot(x, [100 * r['area_reward'] / config['training']['streams'] for r in rows], label=f'seed {seed}')
        axes[0, 1].plot(x, [r['ppo']['epochs'][-1]['value_mae'] for r in rows], label=f'seed {seed}')
        axes[1, 0].plot(x, [r['ppo']['epochs'][-1]['approximate_kl'] for r in rows], label=f'seed {seed}')
        axes[1, 1].plot(x, [r['ppo']['actor_epochs'] for r in rows], label=f'seed {seed}')
    for ax, title, ylabel in zip(axes.flat,
        ('Validated area reward per update', 'Critic error against frozen GAE targets', 'Post-update policy KL', 'Actor epochs before KL stopping'),
        ('Mean gain per stream (% of original area)', 'Mean absolute value error', 'Approximate KL', 'Actor epochs'), strict=True):
        ax.set(title=title, xlabel='PPO update', ylabel=ylabel)
        ax.grid(alpha=.2)
        ax.legend()
    axes[1, 0].axhline(config['ppo']['target_kl'], color='gray', ls=':', label='target')
    axes[1, 1].set_ylim(.75, 4.25)
    fig.suptitle('Persistent blueprint PPO | 32-action chunks, 256-action episodes')
    fig.savefig(run / 'learning.png', dpi=170)
    plt.close(fig)
    fig, axes = plt.subplots(2, len(config['evaluation']['starts']), figsize=(14, 8), constrained_layout=True)
    colors = {seed: color for seed, color in zip(config['seeds'], ('#2379ac', '#bd6127'))}
    for column, start in enumerate(config['evaluation']['starts']):
        ax = axes[0, column]
        for seed in config['seeds']:
            rows = [r for r in grouped if r['seed'] == seed and r['start'] == start]
            ax.plot([r['update'] for r in rows], [r['mean_policy_gain'] for r in rows], 'o-', color=colors[seed], label=f'seed {seed}')
        ax.set(title=start.replace('_', ' '), xlabel='PPO update', ylabel='Policy area improvement (%)')
        ax.set_ylim(bottom=-.5)
        ax.legend()
        ax = axes[1, column]
        for seed in config['seeds']:
            for ordinal in (0, final):
                rows = [r for r in evaluation if r['seed'] == seed and r['start'] == start and r['update'] == ordinal]
                curve = [(0., 0.)]
                for i in range(len(rows[0]['time_curve'])):
                    t = rows[0]['time_curve'][i]['elapsed_ms'] / 1000
                    gain = statistics.mean(100 * (r['start_score'][0] - r['time_curve'][i]['score'][0]) / r['start_score'][0] for r in rows)
                    curve.append((t, gain))
                ax.plot([t for t, _ in curve], [v for _, v in curve], marker='o', color=colors[seed],
                        ls='--' if ordinal == 0 else '-', label=f'{seed}: ' + ('untrained' if ordinal == 0 else 'trained'))
        ax.set(xlabel='Sampled native budget (seconds)', ylabel='Policy area improvement (%)')
        ax.set_ylim(bottom=-.5)
        ax.legend(fontsize=8)
    for ax in axes.flat:
        ax.grid(alpha=.2)
    fig.suptitle('Timed inference | means of two sampling streams per seed/start\nImprovement is relative to the post-prefix valid best; planted gains excluded')
    fig.savefig(run / 'evaluation.png', dpi=170)
    plt.close(fig)
    if first_improvement is not None:
        sample, event = first_improvement
        fig, axes = plt.subplots(1, 2, figsize=(13, 5), constrained_layout=True)
        original = read(Path(__file__).resolve().parents[2] / read(Path(__file__).resolve().parents[2] / config['probe_spec'])['initial'])
        geometries = [original, sample['validated_witness']]
        points = [(p['x'], p['y']) for g in geometries for n in g['transport_networks'] for p in n['cells']]
        points += [(p['x'] + dx, p['y'] + dy) for g in geometries for p in g['placements'] for dx, dy in ((0, 0), (p['width'], p['height']))]
        bounds = (min(x for x, _ in points) - 1, max(x for x, _ in points) + 1,
                  min(y for _, y in points) - 1, max(y for _, y in points) + 1)
        draw(axes[0], original, bounds)
        draw(axes[1], sample['validated_witness'], bounds)
        axes[0].set_title(f'Original valid factory | area {sample["observation"]["initial_metrics"]["score"][0]}')
        axes[1].set_title(f'Policy commit | area {event["score"][0]} | {event["start"]} start')
        qualifier = 'area improvement' if event['reward'] > 0 else 'score improvement'
        fig.suptitle(f'First sampled {qualifier}: seed {event["seed"]}, update {event["update"]}, action {event["action"]}')
        fig.savefig(run / 'first-improvement.png', dpi=170)
        plt.close(fig)
    print(__import__('json').dumps({k: v for k, v in summary.items() if k not in ('episodes', 'training_improvements')}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    args = parser.parse_args()
    summarize(args.run.resolve())


if __name__ == '__main__':
    main()
