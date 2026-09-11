"""Plot every shared-capacity condition and seed against both retained controls."""
import argparse
import json
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np


MODELS = {'shared_40': ('Shared width 40', '#d95f02'),
          'shared_32': ('Shared width 32', '#1f78b4'),
          'separate_32': ('Separate width 32', '#666666')}


def condition_name(condition):
    return ('Critic history' if condition['critic_history'] else 'No critic history') + ' / ' + (
        'critic tail' if condition['critic_after_kl'] else 'joint stop')


def plot(report, output):
    if report.get('schema') != 'ppo-shared-capacity-controls-v1' or report.get('status') != 'complete':
        raise ValueError('expected a completed shared-capacity control comparison')
    conditions = sorted(report['conditions'], key=lambda c: (c['critic_history'], c['critic_after_kl']))
    seeds = sorted(conditions[0]['models']['shared_40']['jobs'], key=int)
    if len(conditions) != 4 or len(seeds) != 2:
        raise ValueError('expected all four conditions and two seeds')
    output.mkdir(parents=True, exist_ok=False)

    figure, axes = plt.subplots(4, 2, figsize=(15, 15), sharex=True, sharey='col')
    for row, condition in enumerate(conditions):
        for column, seed in enumerate(seeds):
            axis = axes[row, column]
            for key, (label, color) in MODELS.items():
                evaluation = condition['models'][key]['jobs'][seed]['evaluations']
                x, y = [e['batch'] for e in evaluation], [e['area_reduction_pp'] for e in evaluation]
                axis.plot(x, y, marker='o', markersize=4, color=color,
                          linewidth=2 if key == 'shared_40' else 1.4, label=f'{label}: final {y[-1]:.2f}%')
            axis.set(title=f'{condition_name(condition)} — seed {seed}', xticks=[0, 8, 16, 24, 32])
            axis.grid(alpha=.2)
            axis.legend(fontsize=8, loc='lower right')
            if column == 0:
                axis.set_ylabel('Best-area reduction (%)')
            if row == len(conditions)-1:
                axis.set_xlabel('Training batch')
    figure.suptitle('Wider shared PPO: every condition and run seed', fontsize=16)
    figure.text(.5, .015, 'Identical factory cohorts within each seed. Width changes the initial policy. Controls are retained earlier runs.',
                ha='center', fontsize=10)
    figure.tight_layout(rect=(0, .035, 1, .97))
    figure.savefig(output/'learning.png', dpi=150, bbox_inches='tight')
    plt.close(figure)

    figure, axes = plt.subplots(2, 3, figsize=(17, 9), sharey=True)
    y = np.arange(len(conditions))
    metrics = (('area_reduction_pp', 'Final area reduction; positive favors wider'),
               ('area_learning_gain_pp', 'Change in gain from initialization'),
               ('critic_late_mae_pp', 'Late critic MAE; negative favors wider'))
    for row, control in enumerate(('shared_32', 'separate_32')):
        label, color = MODELS[control]
        for column, (metric, title) in enumerate(metrics):
            axis = axes[row, column]
            axis.axvline(0, color='black', linewidth=.8, alpha=.6)
            for index, condition in enumerate(conditions):
                result = condition['wider_minus_control'][control]
                values = [result['by_seed'][seed][metric] for seed in seeds]
                axis.plot([min(values), max(values)], [index, index], color=color, alpha=.5)
                for value, marker in zip(values, ('o', 'x')):
                    axis.scatter([value], [index], color=color, marker=marker, s=38)
                axis.scatter([result['mean'][metric]], [index], color='black', marker='|', s=130)
            axis.set(title=f'Wider shared minus {label.lower()}\n{title}', xlabel='Percentage-point difference')
            axis.grid(axis='x', alpha=.2)
    axes[0, 0].set_yticks(y, [condition_name(c) for c in conditions], fontsize=9)
    axes[0, 0].invert_yaxis()
    figure.suptitle('Capacity effects paired within seed and history/schedule condition', fontsize=16)
    figure.text(.5, .025, f'Circle: seed {seeds[0]}; cross: seed {seeds[1]}; black tick: mean. Lines span two observations, not confidence intervals.',
                ha='center', fontsize=10)
    figure.tight_layout(rect=(0, .06, 1, .94))
    figure.savefig(output/'effects.png', dpi=150, bbox_inches='tight')
    plt.close(figure)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    plot(json.loads(args.report.read_text()), args.output.resolve())
