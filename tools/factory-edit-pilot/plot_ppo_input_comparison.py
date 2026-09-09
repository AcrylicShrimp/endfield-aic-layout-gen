"""Plot paired untrained/final held-out outcomes for every input contract and seed."""
import argparse
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from ppo_harness_state import read, require


def plot(summary, output):
    value = read(summary)
    require(value['status'] == 'complete', 'plot-incomplete', 'completed comparison summary required', entity=summary)
    candidates = [c['candidate'] for c in value['candidates']]
    seeds = sorted({j['seed'] for j in value['jobs']})
    figure, axes = plt.subplots(1, 2, figsize=(13, max(3, .5*len(candidates)+1.5)), sharey=True, gridspec_kw=dict(width_ratios=[1.5, 1]), layout='constrained')
    colors = ['#2769a6', '#d66b24']
    cohorts = {}
    for job in value['jobs']:
        if not job['shared_points'] or not job['shared_points'][-1]['shared_cases']:
            continue
        first, last = job['shared_points'][0], job['shared_points'][-1]
        index = seeds.index(job['seed'])
        y = candidates.index(job['candidate']) + (index-(len(seeds)-1)/2)*.28
        color = colors[index % len(colors)]
        a, b = first['mean_area_reduction_percent'], last['mean_area_reduction_percent']
        axes[0].plot([a, b], [y, y], color=color, linewidth=1.5)
        axes[0].scatter([a], [y], facecolors='white', edgecolors=color, s=35, zorder=3)
        axes[0].scatter([b], [y], color=color, s=35, zorder=4)
        axes[1].barh(y, last['delta_from_initial_pp'], height=.21, color=color)
        cohorts[job['seed']] = last['shared_cases']
    axes[0].set_yticks(range(len(candidates)), candidates, fontsize=9)
    axes[0].set_ylim(len(candidates)-.5, -.5)
    axes[0].set_xlim(left=0)
    axes[0].set_xlabel('Retained-best area reduction from initial factory (%)')
    axes[0].set_title('Open marker: untrained policy · filled marker: final policy', fontsize=10)
    axes[1].set_xlabel('Paired change from untrained policy (percentage points)')
    axes[1].set_title('Positive values favor the trained policy', fontsize=10)
    axes[1].axvline(0, color='#555555', linewidth=.8)
    for seed, color in zip(seeds, colors):
        axes[1].plot([], [], color=color, linewidth=4, label=f'Seed {seed} · {cohorts.get(seed, 0)} shared factories')
    axes[1].legend(loc='best', fontsize=8, frameon=False)
    for axis in axes:
        axis.grid(axis='x', alpha=.2)
        axis.set_axisbelow(True)
        axis.spines[['top', 'right']].set_visible(False)
    figure.suptitle('Shallow PPO input comparison · common held-out factories across all candidates/checkpoints', fontsize=12)
    output.mkdir(parents=True, exist_ok=True)
    for extension in ('png', 'svg'):
        figure.savefig(output / f'input-contract-outcomes.{extension}', dpi=160)
    plt.close(figure)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    plot(args.summary, args.output)
    print(f'Rendered paired outcomes under {args.output.resolve()}')
