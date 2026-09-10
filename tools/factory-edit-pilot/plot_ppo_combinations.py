"""Static learning curves and seed-level factorial effects for the PPO screen."""
import argparse
import json
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np


def names(cell):
    f = cell['factors']
    return ' / '.join(('shared' if f['shared_backbone'] else 'separate',
                       'history' if f['critic_history'] else 'no history',
                       'critic tail' if f['critic_after_kl'] else 'joint stop'))


def plot(report, output):
    output.mkdir(parents=True, exist_ok=False)
    cells = sorted(report['cells'], key=lambda c: tuple(c['factors'][k] for k in
                   ('shared_backbone', 'critic_history', 'critic_after_kl')))
    seeds = sorted(cells[0]['jobs'], key=int)
    colors = plt.get_cmap('tab10').colors
    figure, axes = plt.subplots(2, 2, figsize=(16, 11))
    for index, cell in enumerate(cells):
        jobs = [cell['jobs'][seed] for seed in seeds]
        x = [r['batch'] for r in jobs[0]['evaluations']]
        for axis, metric in zip(axes[0], ('area_reduction_pp', 'later_reduction_pp')):
            y = np.asarray([[r[metric] for r in j['evaluations']] for j in jobs])
            axis.plot(x, y.mean(0), marker='o', markersize=3, color=colors[index], label=names(cell))
            axis.fill_between(x, y.min(0), y.max(0), color=colors[index], alpha=.07)
    for axis, title in zip(axes[0], ('Held-out best-area reduction', 'Additional gain from actions 9–32')):
        axis.set(title=title, xlabel='Training batch', ylabel='Percentage points of episode starting area')
        axis.grid(alpha=.2)
    axes[0, 0].legend(fontsize=7, ncols=2, loc='lower right')
    y = np.arange(len(cells))
    summaries = [[c['jobs'][seed] for seed in seeds] for c in cells]
    mae = np.asarray([[j['summary']['critic_mae_pp'] for j in jobs] for jobs in summaries])
    late = np.asarray([[j['summary']['critic_late_mae_pp'] for j in jobs] for jobs in summaries])
    axes[1, 0].barh(y-.16, mae.mean(1), height=.3, label='All positions', color='#377eb8')
    axes[1, 0].barh(y+.16, late.mean(1), height=.3, label='Positions 9–32', color='#ff7f00')
    for values, offset, color in ((mae, -.16, '#174366'), (late, .16, '#9b4d00')):
        axes[1, 0].scatter(values[:, 0], y+offset, s=18, color=color, marker='o', zorder=3)
        axes[1, 0].scatter(values[:, 1], y+offset, s=22, color=color, marker='x', zorder=3)
    axes[1, 0].set(title='Final held-out critic predictive error', xlabel='MAE in reward percentage points; lower is better')
    axes[1, 0].legend(fontsize=8)
    elapsed = np.asarray([[j['summary']['training_seconds']/60 for j in jobs] for jobs in summaries])
    axes[1, 1].barh(y, elapsed.mean(1), color=[colors[i] for i in range(len(cells))], alpha=.75)
    axes[1, 1].scatter(elapsed[:, 0], y, s=20, color='black', marker='o', zorder=3)
    axes[1, 1].scatter(elapsed[:, 1], y, s=25, color='black', marker='x', zorder=3)
    axes[1, 1].set(title='Training phase time per run', xlabel='Minutes, including collection and updates')
    for axis in axes[1]:
        axis.set_yticks(y, [names(c) for c in cells], fontsize=8)
        axis.invert_yaxis()
        axis.grid(axis='x', alpha=.2)
    figure.suptitle('Actor/critic combinations: eight treatments, two run seeds', fontsize=16)
    figure.text(.5, .015, f'Lines/bars are seed means; bands span the two seeds. Circles: seed {seeds[0]}; crosses: seed {seeds[1]}. No confidence intervals.',
                ha='center', fontsize=9)
    figure.tight_layout(rect=(0, .035, 1, .96))
    figure.savefig(output/'overview.png', dpi=150, bbox_inches='tight')
    plt.close(figure)

    labels = {'shared_backbone': 'Shared backbone', 'critic_history': 'Critic history', 'critic_after_kl': 'Critic tail'}
    effects = report['effects']
    figure, axes = plt.subplots(1, 3, figsize=(15, 6), sharey=True)
    y = np.arange(len(effects))
    for axis, metric, title in zip(axes, ('area_reduction_pp', 'later_reduction_pp', 'critic_late_mae_pp'),
            ('Best-area reduction: higher is better', 'Additional gain from actions 9–32', 'Late critic MAE: lower is better')):
        axis.axvline(0, color='gray', linewidth=1)
        for index, effect in enumerate(effects):
            values = [effect['by_seed'][seed][metric] for seed in seeds]
            axis.plot([min(values), max(values)], [index, index], color='#377eb8', alpha=.5)
            for value, marker in zip(values, ('o', 'x')):
                axis.scatter([value], [index], color='#377eb8', s=25, marker=marker)
            axis.scatter([effect['mean'][metric]], [index], color='black', marker='|', s=100)
        axis.set(title=title, xlabel='Treatment difference, percentage points')
        axis.grid(axis='x', alpha=.2)
    axes[0].set_yticks(y, [' × '.join(labels[f] for f in e['factors']) for e in effects], fontsize=9)
    axes[0].invert_yaxis()
    figure.suptitle('Paired factorial effects and interactions', fontsize=15)
    figure.text(.5, .025, f'Circles: seed {seeds[0]}; crosses: seed {seeds[1]}, each averaged over matched contexts. Black ticks: means. Two seeds are exploratory.',
                ha='center', fontsize=9)
    figure.tight_layout(rect=(0, .05, 1, .94))
    figure.savefig(output/'effects.png', dpi=150, bbox_inches='tight')
    plt.close(figure)


def plot_critic_probe(report, output):
    batches = report['batches']
    figure, axes = plt.subplots(2, len(batches), figsize=(5*len(batches), 8), squeeze=False)
    for column, batch in enumerate(batches):
        positions = batch['positions']
        x = [p['action'] for p in positions]
        axis = axes[0, column]
        axis.plot(x, [100*p['before']['mean_return'] for p in positions], color='black', label='Realized return')
        for key, label, color in (('before', 'Before extra critic fit', '#377eb8'),
                                  ('after', 'After extra critic fit', '#ff7f00')):
            axis.plot(x, [100*p[key]['mean_prediction'] for p in positions], color=color, label=label)
            axes[1, column].plot(x, [100*p[key]['mae'] for p in positions], color=color, label=label)
        axis.set(title=f"Original batch {batch['batch']}; {batch['held_out_accepted']} held-out factories",
                 ylabel='Mean remaining reward, percentage points')
        axes[1, column].set(ylabel='Predictive MAE, percentage points', xlabel='Action position')
        for axis in axes[:, column]:
            axis.axvline(8.5, color='gray', linestyle=':', linewidth=1)
            axis.set_xticks([1, 8, 16, 24, 32])
            axis.grid(alpha=.2)
    axes[0, 0].legend(fontsize=8)
    figure.suptitle('Mature critics: additional fitting on identical held-out trajectories', fontsize=15)
    figure.text(.5, .02, 'Actors remain frozen. Three snapshots share one training lineage; curves are descriptive means, not independent replications.',
                ha='center', fontsize=9)
    figure.tight_layout(rect=(0, .04, 1, .95))
    figure.savefig(output/'mature-critic.png', dpi=150, bbox_inches='tight')
    plt.close(figure)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--critic-probe', type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    if report.get('schema') != 'ppo-actor-critic-comparison-v1':
        parser.error('expected a completed actor/critic comparison report')
    probe = json.loads(args.critic_probe.read_text()) if args.critic_probe else None
    if probe is not None and (probe.get('schema') != 'ppo-mature-critic-tail-v1' or probe.get('status') != 'complete'):
        parser.error('expected a completed mature-critic probe report')
    plot(report, args.output.resolve())
    if probe is not None:
        plot_critic_probe(probe, args.output.resolve())
