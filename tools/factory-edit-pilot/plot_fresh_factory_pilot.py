"""Plot audited parallel throughput and matched learning evaluation checkpoints."""
import argparse
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(args):
    summary = json.loads(args.summary.read_text())
    if summary.get('status') != 'complete' or (summary.get('smoke') and not args.allow_smoke):
        raise ValueError('fresh-plot.expected-audited-summary: smoke requires explicit flag')
    args.output.mkdir()
    cells = sorted((c for c in summary['scaling'] if c['status'] == 'complete'), key=lambda c: c['workers'])
    with plt.rc_context({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False}):
        figure, axes = plt.subplots(1, 2, figsize=(12, 4.8))
        axis = axes[0]
        axis.plot([c['workers'] for c in cells], [c['valid_actions_per_second'] for c in cells],
                  marker='o', color='#2865a5', label='Combined rate')
        for index, cell in enumerate(cells):
            axis.scatter([cell['workers']] * len(cell['repetition_actions_per_second']),
                         cell['repetition_actions_per_second'], color='#999999', s=18,
                         label='Individual batches' if index == 0 else None)
        axis.set(xlabel='Concurrent CPU workers', ylabel='Valid policy actions / wall second',
                 title='Fresh generation and collection throughput')
        axis.set_xticks([c['workers'] for c in cells])
        axis.set_ylim(bottom=0)
        axis.legend(frameon=False)
        axis.grid(axis='y', alpha=.15)
        axis = axes[1]
        axis.axhline(0, color='#888888', linewidth=.8)
        for seed in summary['learning']:
            points = seed['evaluation']
            axis.plot([p['update'] for p in points],
                      [p['products']['all']['paired_area_delta_pp'] for p in points],
                      marker='o', label=f"Seed {seed['seed']}")
        axis.set(xlabel='Completed optimizer updates',
                 ylabel='Additional area reduction vs. initial policy (pp)',
                 title='Evaluation on matched fresh factory draws')
        axis.set_xticks([p['update'] for p in summary['learning'][0]['evaluation']])
        axis.grid(axis='y', alpha=.15)
        axis.legend(frameon=False)
        figure.suptitle('Fresh-factory pilot' + (' — SMOKE EXECUTION CHECK' if summary['smoke'] else ''), fontsize=15)
        figure.text(.5, .01, 'Throughput includes rejected generation attempts; process startup is separate. '
                    'Evaluation includes all admitted draws from four configured products.',
                    ha='center', fontsize=8)
        figure.tight_layout(rect=(0, .04, 1, .95))
        for extension in ('png', 'svg'):
            figure.savefig(args.output / f'pilot.{extension}', dpi=160)
        plt.close(figure)
    manifest = dict(summary_sha256=sha(args.summary), plot_source_sha256=sha(__file__),
                    outputs={name: sha(args.output / name) for name in ('pilot.png', 'pilot.svg')})
    (args.output / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(dict(status='complete', output=str(args.output))))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--allow-smoke', action='store_true')
    run(parser.parse_args())
