"""Continuously publish measured direct-area performance, separate from training rewards."""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
from statistics import mean
import time

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from full_factory_screen import sha
from random_factory_generation import read, write


def paired(evaluation, baseline):
    before = {r['identity']['index']: r for r in baseline['results']}
    after = {r['identity']['index']: r for r in evaluation['results']}
    if len(before) != len(baseline['results']) or len(after) != len(evaluation['results']) or before.keys() != after.keys():
        raise ValueError('direct-reward.comparison: evaluation bank differs')
    cases = []
    for index, result in sorted(after.items()):
        original = before[index]
        if result['status'] != original['status']:
            raise ValueError('direct-reward.comparison: admission differs')
        if result['status'] != 'complete':
            continue
        if (result['generation']['initial_sha256'] != original['generation']['initial_sha256'] or
                result['generation']['request_sha256'] != original['generation']['request_sha256']):
            raise ValueError('direct-reward.comparison: initial factory differs')
        initial = result['row']['initial_metrics']['score']
        a, b = original['row']['best_metrics']['score'], result['row']['best_metrics']['score']
        cases.append(dict(index=index, product=result['generation']['draw']['product'], initial_score=initial,
            baseline_score=a, score=b, outcome='wins' if b < a else 'losses' if b > a else 'ties',
            area_reduction_percent=100 * (initial[0] - b[0]) / initial[0],
            delta_pp=100 * (a[0] - b[0]) / initial[0], transport_delta=a[1] - b[1], turn_delta=a[2] - b[2]))
    if not cases:
        raise ValueError('direct-reward.comparison: no admitted evaluation cases')
    groups = defaultdict(list)
    for case in cases:
        groups['all'].append(case)
        groups[case['product']].append(case)
    return dict(cases=cases, products={name: dict(samples=len(rows),
        area_reduction_percent=mean(r['area_reduction_percent'] for r in rows),
        delta_pp=mean(r['delta_pp'] for r in rows), **dict(Counter(r['outcome'] for r in rows)))
        for name, rows in groups.items()})


def checked(entry):
    if sha(entry['path']) != entry['sha256']:
        raise ValueError(f'direct-reward.comparison: hash mismatch {entry["path"]}')
    return read(entry['path'])


def snapshot(screen):
    protocol = read(screen / 'protocol.json')
    progress = read(screen / 'progress.json')
    if progress['protocol_sha256'] != sha(screen / 'protocol.json'):
        raise ValueError('direct-reward.comparison: protocol mismatch')
    seeds = []
    for state in progress['seeds']:
        evaluations = [checked(e) for e in state['evaluations']]
        if not evaluations:
            continue
        reference = checked(protocol['reference'][str(state['seed'])])
        baseline = evaluations[0] if protocol['smoke'] else reference['evaluations'][0]
        points = [dict(update=e['update'], selected=e['selected_training_actions'],
            elapsed_seconds=e['elapsed_seconds'], measured_epoch=e['measured_epoch'], **paired(e, baseline)) for e in evaluations]
        historical = [] if protocol['smoke'] else [dict(update=e['update'], **paired(e, baseline)) for e in reference['evaluations']]
        seeds.append(dict(seed=state['seed'], status=state['status'], points=points, rank_reference=historical))
    now = time.time()
    return dict(schema='direct-area-performance-curve-v1', status=progress['status'], smoke=protocol['smoke'],
        updated_epoch=now, last_measurement_age_seconds=now - max((p['measured_epoch'] for s in seeds for p in s['points']), default=now),
        training_updates=progress['updates'], training_actions=progress['selected'], seeds=seeds,
        protocol_sha256=sha(screen / 'protocol.json'), progress_sha256=sha(screen / 'progress.json'),
        diagnostic=progress.get('diagnostic'),
        interpretation='Measured matched-factory deltas; lines connect checkpoints, no smoothing or extrapolation. Historical rank has only 0/16/32 measurements.')


def publish(screen, output):
    value = snapshot(screen)
    output.mkdir(parents=True, exist_ok=True)
    with plt.rc_context({'font.size': 10, 'axes.spines.top': False, 'axes.spines.right': False}):
        figure, axes = plt.subplots(1, 3, figsize=(16, 5))
        for axis in axes:
            axis.axhline(0, color='#888888', linewidth=.8)
            axis.grid(axis='y', alpha=.15)
        for i, seed in enumerate(value['seeds']):
            color = ['#2865a5', '#c15b22'][i % 2]
            points = seed['points']
            delta = [p['products']['all']['delta_pp'] for p in points]
            label = f"Direct area, seed {seed['seed']}"
            axes[0].plot([p['elapsed_seconds'] / 60 for p in points], delta, 'o-', color=color, label=label)
            axes[1].plot([p['selected'] for p in points], delta, 'o-', color=color, label=label)
            axes[2].plot([p['update'] for p in points], delta, 'o-', color=color, label=label)
            reference = seed['rank_reference']
            if reference:
                axes[2].plot([p['update'] for p in reference], [p['products']['all']['delta_pp'] for p in reference],
                             'x--', color=color, alpha=.65, label=f"Historical rank, seed {seed['seed']}")
        axes[0].set(xlabel='Elapsed minutes since this seed started (including evaluation/audit)',
                    ylabel='Additional area reduction vs. untrained policy (pp)', title='Performance over elapsed time')
        axes[1].set(xlabel='Selected training actions', title='Performance over experience')
        axes[2].set(xlabel='Completed optimizer updates', title='Matched historical rank comparison')
        for axis in axes:
            if value['seeds']:
                axis.legend(frameon=False, fontsize=8)
        figure.suptitle('Direct area reward learning' + (' — SMOKE CHECK' if value['smoke'] else ''), fontsize=16)
        latest = ', '.join(f"seed {s['seed']}: {s['points'][-1]['products']['all']['delta_pp']:+.4f} pp"
                           for s in value['seeds']) or 'Waiting for first completed evaluation'
        figure.text(.5, .045, f"{value['status']} | {value['training_updates']} updates | {latest}", ha='center', fontsize=9)
        figure.text(.5, .015, f"Last measurement age: {value['last_measurement_age_seconds']:.0f}s. "
                    'Refreshes are not new measurements. Each seed uses its own matched evaluation bank.', ha='center', fontsize=8)
        figure.tight_layout(rect=(0, .08, 1, .95))
        for extension in ('png', 'svg'):
            temporary = output / f'performance.tmp.{extension}'
            figure.savefig(temporary, dpi=150)
            temporary.replace(output / f'performance.{extension}')
        plt.close(figure)
    write(output / 'curve.json', value)
    write(output / 'manifest.json', dict(status=value['status'], updated_epoch=value['updated_epoch'],
        plot_source_sha256=sha(__file__), outputs={name: sha(output / name) for name in ('performance.png', 'performance.svg', 'curve.json')}))
    return value


def main(args):
    deadline = time.monotonic() + args.seconds
    while True:
        if (args.screen / 'progress.json').is_file():
            value = publish(args.screen, args.output)
            print(json.dumps(dict(status=value['status'], refreshed_epoch=value['updated_epoch'],
                                  measured_points=sum(len(s['points']) for s in value['seeds']))), flush=True)
            if not args.watch or value['status'] != 'running':
                return
        elif not args.watch:
            raise ValueError('direct-reward.plot-failed: no progress file')
        if time.monotonic() >= deadline:
            raise TimeoutError('direct-reward.plot-failed: observer time budget')
        # New completed measurements are published promptly; otherwise refresh once per minute.
        previous = (args.screen / 'progress.json').stat().st_mtime_ns if (args.screen / 'progress.json').exists() else None
        until = min(deadline, time.monotonic() + args.refresh_seconds)
        while time.monotonic() < until:
            time.sleep(min(2, max(0, until - time.monotonic())))
            if (args.screen / 'progress.json').exists() and (args.screen / 'progress.json').stat().st_mtime_ns != previous:
                break


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--watch', action='store_true')
    parser.add_argument('--seconds', type=float, default=7800)
    parser.add_argument('--refresh-seconds', type=float, default=60)
    args = parser.parse_args()
    if not 0 < args.seconds <= 86400 or not 1 <= args.refresh_seconds <= 60:
        parser.error('observer seconds must be (0, 86400], refresh seconds [1, 60]')
    try:
        main(args)
    except BaseException as error:
        args.output.mkdir(parents=True, exist_ok=True)
        write(args.output / 'observer-error.json', dict(code='direct-reward.plot-failed', severity='error',
            stage='performance-observer', entity=str(args.screen), message=str(error)))
        raise
