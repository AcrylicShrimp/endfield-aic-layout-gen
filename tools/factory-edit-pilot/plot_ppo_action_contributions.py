"""Plot an action contribution audit using an environment with matplotlib."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    runs = json.loads(args.report.read_text())['runs']
    intervals = sorted(k for k in runs['new'] if k[0].isdigit())
    x = [int(k.split('-')[1]) for k in intervals]
    fig, axes = plt.subplots(1, 2, figsize=(12, 4.5), layout='constrained')
    fig.suptitle('Where area improvements come from', fontsize=17, fontweight='bold')
    fig.set_facecolor('#fafafa')
    for axis in axes:
        axis.set_facecolor('#fafafa')
        axis.spines[['top', 'right']].set_visible(False)
        axis.grid(axis='y', alpha=.18)
        axis.set_xlabel('Cumulative training batches (8-evaluation windows)')
        axis.set_xticks(x[::2] + [x[-1]])
    contributions = [[runs['new'][k]['kinds'][kind]['mean_contribution_pp']
                      for k in intervals] for kind in ['contract', 'pose']]
    axes[0].stackplot(x, *contributions, labels=['New: contract', 'New: pose'],
                      colors=['#397eaf', '#f0ac47'], alpha=.9)
    axes[0].plot(x, [runs['old'][k]['mean_area_reduction_percent'] for k in intervals],
                 color='#292929', linestyle='--', linewidth=2, label='Previous: total')
    axes[0].set_title('Direct contribution to final best-area reduction')
    axes[0].set_ylabel('Percentage points of initial area')
    axes[0].set_ylim(0, 70)
    axes[0].legend(loc='lower right', frameon=False)
    for name, label, color in [('old', 'Previous', '#777777'), ('new', 'New', '#177e89')]:
        values = [sum(c['mean_contribution_pp'] for p, c in runs[name][k]['positions'].items()
                      if p != '01-08') for k in intervals]
        axes[1].plot(x, values, 'o-', label=label, color=color, linewidth=2, markersize=4)
        axes[1].annotate(f'{values[-1]:.2f} pp', (x[-1], values[-1]),
                         xytext=(-45, 10), textcoords='offset points', color=color)
    axes[1].set_title('Additional area reduction in actions 9-32')
    axes[1].set_ylabel('Percentage points of initial area')
    axes[1].set_ylim(bottom=0)
    axes[1].legend(frameon=False)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output, dpi=170, facecolor=fig.get_facecolor())
    plt.close(fig)


if __name__ == '__main__':
    main()
