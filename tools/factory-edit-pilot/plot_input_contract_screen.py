"""Plot registered input-screen checkpoints and both seed endpoints."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from full_factory_screen import sha

LABELS = {
    'action-only': 'Action features only',
    'geometry-current': 'Current geometry',
    'full-current': 'Full current',
    'current-history-4': 'Current + 4 actions',
    'current-history-16': 'Current + 16 actions',
    'current-history-16-outcomes': 'Current + 16 actions / outcomes',
    'current-best-2': 'Current + 2 best scores',
    'current-best-3': 'Current + 3 best scores',
    'current-best-layout': 'Current + best layout',
    'current-best-layout-history': 'Best layout + action history',
    'current-no-restore': 'Current / restore disabled',
    'history-no-restore': 'History / restore disabled',
}


def run(args):
    summary = json.loads(args.summary.read_text())
    if summary.get('status') != 'complete' or summary.get('smoke'):
        raise ValueError('input-plot.expected-complete-nonsmoke-summary')
    args.output.mkdir()
    with plt.rc_context({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False}):
        figure, axes = plt.subplots(3, 4, figsize=(16, 10), sharex=True, sharey=True)
        for axis, candidate in zip(axes.flat, summary['candidates']):
            points = candidate['curve']
            axis.axhline(0, color='#888888', linewidth=.8)
            axis.plot([p['update'] for p in points], [p['paired_area_delta_pp'] for p in points],
                      marker='o', color='#2563a6', linewidth=2)
            for i, seed in enumerate(candidate['per_seed']):
                axis.scatter(points[-1]['update'] + (i - .5) * .7, seed['paired_area_delta_pp'],
                             color=('#d67b23', '#5a9b52')[i % 2], marker=('s', '^')[i % 2], s=35, zorder=3)
            end = points[-1]
            axis.set_title(LABELS[candidate['candidate']], fontsize=10)
            axis.text(.03, .95, f"Score W/T/L: {end['wins']}/{end['ties']}/{end['losses']}",
                      transform=axis.transAxes, va='top', fontsize=9)
            axis.set_xticks([p['update'] for p in points])
            axis.grid(axis='y', alpha=.15)
        figure.supxlabel('Completed optimizer updates (128 selected training actions per update)')
        figure.supylabel('Mean area improvement versus own initialization (percentage points)')
        figure.suptitle('Input-contract screen: learning checkpoints, not convergence estimates', fontsize=15)
        figure.text(.5, .025, 'Blue: mean of both seeds. Orange square / green triangle: seed endpoints. '
                    'W/T/L compares the complete lexicographic score. Previously opened controls are included.',
                    ha='center', fontsize=9)
        figure.tight_layout(rect=(.02, .055, 1, .95))
        figure.savefig(args.output / 'learning-curves.png', dpi=160)
        figure.savefig(args.output / 'learning-curves.svg')
        plt.close(figure)
    manifest = dict(schema_version=1, summary=str(args.summary.resolve()), summary_sha256=sha(args.summary),
                    plot_source_sha256=sha(__file__), outputs={name: sha(args.output / name)
                    for name in ('learning-curves.png', 'learning-curves.svg')})
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(dict(status='complete', output=str(args.output)), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
