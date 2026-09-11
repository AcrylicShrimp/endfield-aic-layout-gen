"""Plot paired learning-rate quality, critic error and actual PPO update behavior."""
import argparse
import json
from pathlib import Path

import matplotlib.pyplot as plt


COLORS = {0.0003: '#666666', 0.0001: '#1f78b4', 0.00003: '#d95f02'}


def plot(report, output):
    if report.get('schema') != 'ppo-learning-rate-controls-v1' or report.get('status') != 'complete':
        raise ValueError('expected a completed learning-rate control comparison')
    cells = sorted([report['baseline'], *report['cells']], key=lambda c: -c['learning_rate'])
    if {c['learning_rate'] for c in cells} != set(COLORS):
        raise ValueError('expected the registered three learning rates')
    seeds = sorted(cells[0]['jobs'], key=int)
    if len(seeds) != 2:
        raise ValueError('expected both run seeds')
    output.mkdir(parents=True, exist_ok=False)
    figure, axes = plt.subplots(2, 3, figsize=(17, 10))
    for row, seed in enumerate(seeds):
        for cell in cells:
            rate = cell['learning_rate']
            label = f'{rate:g}' + (' (retained control)' if rate == 0.0003 else '')
            color = COLORS[rate]
            evaluation = cell['jobs'][seed]['evaluations']
            batch = [e['batch'] for e in evaluation]
            for column, key in enumerate(('area_reduction_pp', 'critic_late_mae_pp')):
                axes[row, column].plot(batch, [e[key] for e in evaluation], marker='o', markersize=4,
                                       color=color, label=label)
            updates = cell['optimization'][seed]['by_batch']
            axes[row, 2].plot([u['batch'] for u in updates], [u['actor_epochs'] for u in updates],
                              color=color, alpha=.8, linewidth=1.2, label=label)
        for column, title in enumerate(('Best-area reduction; higher is better', 'Late critic MAE; actions 9–32', 'Actor epochs per batch')):
            axis = axes[row, column]
            axis.set(title=f'Seed {seed}: {title}', xlabel='Training batch', xticks=[0, 8, 16, 24, 32])
            axis.grid(alpha=.2)
            axis.legend(fontsize=8)
        axes[row, 0].set_ylabel('Area reduction (%)')
        axes[row, 1].set_ylabel('MAE (percentage points)')
        axes[row, 2].set(yticks=[1, 2, 3, 4], ylim=(.8, 4.2), ylabel='Completed epochs')
    figure.suptitle('Lower learning rates: shared width 40, critic history and critic tail', fontsize=16)
    figure.text(.5, .02, 'Same initial policies and factory cohorts within each seed. Critic errors follow each policy trajectory.',
                ha='center', fontsize=10)
    figure.tight_layout(rect=(0, .04, 1, .96))
    figure.savefig(output/'learning.png', dpi=150, bbox_inches='tight')
    plt.close(figure)

    figure, axes = plt.subplots(2, 2, figsize=(14, 10))
    for row, seed in enumerate(seeds):
        for cell in cells:
            rate = cell['learning_rate']
            updates = cell['optimization'][seed]['by_batch']
            batches = [u['batch'] for u in updates]
            axes[row, 0].plot(batches, [u['first_epoch_kl'] for u in updates], color=COLORS[rate], label=f'{rate:g}')
            axes[row, 1].plot(batches, [100*u['final_epoch_outside_clip_fraction'] for u in updates],
                              color=COLORS[rate], label=f'{rate:g}')
        axes[row, 0].axhline(report['ppo_control']['target_kl'], color='black', linestyle='--', linewidth=1, label='KL stop threshold')
        axes[row, 0].set(title=f'Seed {seed}: policy KL after the first epoch', ylabel='Approximate KL', yscale='log')
        axes[row, 1].set(title=f'Seed {seed}: outside clipping range after final actor epoch', ylabel='Sampled actions (%)')
        for axis in axes[row]:
            axis.set(xlabel='Training batch', xticks=[1, 8, 16, 24, 32])
            axis.grid(alpha=.2)
            axis.legend(fontsize=9)
    figure.suptitle('Learning-rate effects on policy movement', fontsize=16)
    figure.text(.5, .02, 'Final actor epochs can differ under KL stopping. Outside-range fraction is distinct from objective-clipped fraction.',
                ha='center', fontsize=10)
    figure.tight_layout(rect=(0, .04, 1, .96))
    figure.savefig(output/'updates.png', dpi=150, bbox_inches='tight')
    plt.close(figure)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    plot(json.loads(args.report.read_text()), args.output.resolve())
