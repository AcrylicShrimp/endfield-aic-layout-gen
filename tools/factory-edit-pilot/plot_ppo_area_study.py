"""Measured PPO performance with matched direct/rank references and critic/ratio diagnostics."""
import argparse
import json
from pathlib import Path
import time

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from full_factory_screen import sha
from plot_direct_area_study import checked, paired
from random_factory_generation import read, write


def snapshot(screen):
    protocol = read(screen / 'protocol.json')
    progress = read(screen / 'progress.json')
    if progress['protocol_sha256'] != sha(screen / 'protocol.json'):
        raise ValueError('ppo-area.observer-failed: protocol mismatch')
    seeds = []
    for state in progress['seeds']:
        evaluations = [checked(e) for e in state['evaluations']]
        if not evaluations:
            continue
        reference = checked(protocol['reference'][str(state['seed'])])
        direct = [checked(e) for e in reference['evaluations']]
        baseline = evaluations[0] if protocol['smoke'] else direct[0]
        def point(e):
            return dict(update=e['update'], selected=e['selected_training_actions'],
                elapsed_seconds=e['elapsed_seconds'], measured_epoch=e['measured_epoch'], **paired(e, baseline))
        rank = checked(protocol['rank_reference'][str(state['seed'])])
        seeds.append(dict(seed=state['seed'], status=state['status'], points=[point(e) for e in evaluations],
            direct_reference=[] if protocol['smoke'] else [point(e) for e in direct],
            rank_reference=[] if protocol['smoke'] else [dict(update=e['update'], **paired(e, baseline)) for e in rank['evaluations']],
            diagnostics=state['diagnostics']))
    now = time.time()
    return dict(schema='ppo-area-performance-curve-v1', status=progress['status'], smoke=protocol['smoke'],
        updated_epoch=now, last_measurement_age_seconds=now - max((p['measured_epoch'] for s in seeds for p in s['points']), default=now),
        rollout_batches=progress['updates'], training_actions=progress['selected'], seeds=seeds,
        protocol_sha256=sha(screen / 'protocol.json'), diagnostic=progress.get('diagnostic'),
        clip_epsilon=protocol['controls']['ppo']['clip_epsilon'], target_kl=protocol['controls']['ppo']['target_kl'])


def publish(screen, output):
    value = snapshot(screen)
    output.mkdir(parents=True, exist_ok=True)
    with plt.rc_context({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False}):
        figure, grid = plt.subplots(2, 3, figsize=(16, 8.5))
        axes = grid.flatten()
        for axis in axes:
            axis.grid(axis='y', alpha=.15)
        for axis in (*axes[:3], axes[4]):
            axis.axhline(0, color='#999999', linewidth=.8)
        axes[3].axhline(1, color='#999999', linewidth=.8)
        for bound in (1 - value['clip_epsilon'], 1 + value['clip_epsilon']):
            axes[3].axhline(bound, color='#999999', linestyle=':', linewidth=.8)
        axes[5].axhline(value['target_kl'], color='#999999', linestyle=':', linewidth=.8)
        for index, seed in enumerate(value['seeds']):
            color = ['#2865a5', '#c15b22'][index % 2]
            points = seed['points']
            delta = [p['products']['all']['delta_pp'] for p in points]
            label = f"PPO {seed['seed']}"
            axes[0].plot([p['elapsed_seconds'] / 60 for p in points], delta, 'o-', color=color, label=label)
            axes[1].plot([p['selected'] for p in points], delta, 'o-', color=color, label=label)
            axes[2].plot([p['update'] for p in points], delta, 'o-', color=color, label=label)
            direct = seed['direct_reference']
            if direct:
                values = [p['products']['all']['delta_pp'] for p in direct]
                axes[1].plot([p['selected'] for p in direct], values, 'x--', color=color, alpha=.65, label=f"Direct {seed['seed']}")
                axes[2].plot([p['update'] for p in direct], values, 'x--', color=color, alpha=.65, label=f"Direct {seed['seed']}")
            rank = seed['rank_reference']
            if rank:
                axes[2].plot([p['update'] for p in rank], [p['products']['all']['delta_pp'] for p in rank],
                             '+:', color=color, alpha=.5, label=f"Rank {seed['seed']}")
            diagnostics = seed['diagnostics']
            if diagnostics:
                updates = [d['update'] for d in diagnostics]
                axes[3].plot(updates, [d['ratio_mean'] for d in diagnostics], color=color, label=label)
                axes[3].fill_between(updates, [d['ratio_p05'] for d in diagnostics], [d['ratio_p95'] for d in diagnostics], color=color, alpha=.15)
                axes[4].plot(updates, [float('nan') if d['explained_variance'] is None else d['explained_variance'] for d in diagnostics], color=color, label=label)
                axes[5].plot(updates, [d['approximate_kl'] for d in diagnostics], color=color, label=label)
        axes[0].set(title='Performance over elapsed time', xlabel='Minutes in this seed (including evaluation/audit)',
                    ylabel='Additional area reduction vs. untrained policy (pp)')
        axes[1].set(title='Performance over experience', xlabel='Selected training actions')
        axes[2].set(title='Matched learning comparisons', xlabel='Completed rollout batches')
        axes[3].set(title='Actual probability ratio: mean and 5–95% band', xlabel='Completed rollout batches', ylabel='New / behavior action probability')
        axes[4].set(title='Critic explained variance on frozen GAE targets', xlabel='Completed rollout batches')
        axes[5].set(title='Whole-batch sampled KL after final epoch', xlabel='Completed rollout batches')
        for axis in axes:
            if axis.get_legend_handles_labels()[0]:
                axis.legend(frameon=False, fontsize=8)
        figure.suptitle('PPO + critic + GAE, direct area reward' + (' — SMOKE CHECK' if value['smoke'] else ''), fontsize=16)
        latest = ', '.join(f"seed {s['seed']}: {s['points'][-1]['products']['all']['delta_pp']:+.4f} pp" for s in value['seeds']) or 'Waiting for first evaluation'
        figure.text(.5, .035, f"{value['status']} | {value['rollout_batches']} rollout batches | {latest}", ha='center', fontsize=9)
        figure.text(.5, .012, f"Last measurement age: {value['last_measurement_age_seconds']:.0f}s. "
            'Lines connect measured checkpoints; ratio clipping is an objective term, not a hard ratio bound.', ha='center', fontsize=8)
        figure.tight_layout(rect=(0, .065, 1, .955))
        for extension in ('png', 'svg'):
            temporary = output / f'performance.tmp.{extension}'
            figure.savefig(temporary, dpi=150)
            temporary.replace(output / f'performance.{extension}')
        plt.close(figure)
    write(output / 'curve.json', value)
    write(output / 'manifest.json', dict(status=value['status'], updated_epoch=value['updated_epoch'],
        plot_source_sha256=sha(__file__), outputs={name: sha(output / name) for name in ('performance.png', 'performance.svg', 'curve.json')}))
    return value


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--watch', action='store_true')
    parser.add_argument('--seconds', type=float, default=15000)
    args = parser.parse_args()
    if not 0 < args.seconds <= 86400:
        parser.error('observer seconds must be in (0, 86400]')
    deadline = time.monotonic() + args.seconds
    try:
        while True:
            if (args.screen / 'progress.json').exists():
                result = publish(args.screen, args.output)
                print(json.dumps(dict(status=result['status'], refreshed_epoch=result['updated_epoch'],
                    measured_points=sum(len(s['points']) for s in result['seeds']))), flush=True)
                if not args.watch or result['status'] != 'running':
                    break
            elif not args.watch:
                raise ValueError('ppo-area.observer-failed: no progress')
            if time.monotonic() >= deadline:
                raise TimeoutError('ppo-area.observer-failed: time budget')
            time.sleep(min(30, deadline - time.monotonic()))
    except BaseException as error:
        args.output.mkdir(parents=True, exist_ok=True)
        write(args.output / 'observer-error.json', dict(code='ppo-area.observer-failed', severity='error',
            stage='performance-observer', entity=str(args.screen), message=str(error)))
        raise
