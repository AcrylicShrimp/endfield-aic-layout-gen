"""Compare audited PPO/BC arms without changing the registered experiment."""
import argparse
from collections import Counter, defaultdict
import math
from pathlib import Path
import statistics

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from blueprint_ppo_store import ROOT, checked, load, read, reference, sha, write
from blueprint_ppo_update import digest, require
from plot_blueprint_edit import draw


def summarize_arm(run, label):
    protocol, state, audit = [read(run / f'{name}.json') for name in ('protocol', 'state', 'audit')]
    completion, completion_audit = [read(run / 'completion' / f'{name}.json') for name in ('report', 'audit')]
    require(audit['status'] == completion_audit['status'] == 'passed', 'blueprint.bc-summary: passing audits')
    require(audit['protocol_sha256'] == completion['protocol_sha256'] == sha(run / 'protocol.json'), 'blueprint.bc-summary: protocol binding')
    require(completion_audit['report_sha256'] == sha(run / 'completion/report.json'), 'blueprint.bc-summary: completion audit binding')
    config, evaluation = protocol['config'], audit['evaluation']
    series = []
    action_curves = []
    for seed in config['seeds']:
        for ordinal in config['evaluation']['at']:
            rows = [r for r in evaluation if r['seed'] == seed and r['update'] == ordinal]
            counts = Counter()
            for row in rows:
                counts.update(row['counts'])
            actions = sum(r['policy_actions'] for r in rows)
            series.append(dict(seed=seed, update=ordinal, gain=statistics.mean(r['policy_area_reduction_percent'] for r in rows),
                pose_percent=100 * counts['kind/pose'] / actions, actions=actions,
                by_start={start: statistics.mean(r['policy_area_reduction_percent'] for r in rows if r['start'] == start)
                          for start in config['evaluation']['starts']}))
            for milestone in config['evaluation']['action_milestones']:
                gains = [100 * (r['start_score'][0] - point['best_score'][0]) / r['start_score'][0]
                         for r in rows for point in r['action_milestones'] if point['policy_action'] == milestone]
                action_curves.append(dict(seed=seed, update=ordinal, actions=milestone, cases=len(gains),
                                          mean_gain=statistics.mean(gains) if gains else None))
    updates, episodes, improvements = [], [], []
    first = None
    kinds = Counter()
    accepted_poses = defaultdict(int)
    for seed in config['seeds']:
        for ref in state['jobs'][str(seed)]['records']:
            record = read(checked(ref, run))
            updates.append(dict(seed=seed, update=record['update'], actor_epochs=record['ppo']['actor_epochs'],
                bc=record['ppo']['bc'], final_kl=record['ppo']['epochs'][-1]['approximate_kl'],
                update_seconds=record['ppo']['seconds'], collection_seconds=record['collection_seconds'], reward=record['area_reward']))
            chunk = load(checked(record['chunk'], run))
            for sample in chunk['samples']:
                action = sample['input']['actions'][sample['index']]
                kinds[action['kind']] += 1
                key = (seed, sample['stream'], sample['episode'])
                if action['kind'] == 'pose' and sample['outcome']['status'] != 'rejected':
                    accepted_poses[key] += 1
                if sample['reward'] > 0:
                    event = dict(seed=seed, update=record['update'], stream=sample['stream'], episode=sample['episode'],
                        start=sample['start'], action=sample['policy_action'], reward=sample['reward'],
                        score=sample['outcome']['best_metrics']['score'], preceding_poses=accepted_poses[key], chunk=record['chunk'])
                    improvements.append(event)
                    if first is None and sample['start'] in ('original', 'overlap') and accepted_poses[key]:
                        first = (sample['validated_witness'], event)
            for episode in chunk['completed']:
                episodes.append(dict(seed=seed, stream=episode['stream'], episode=episode['episode'], start=episode['start'],
                    start_score=episode['start_best'], best_score=episode['result']['best_metrics']['score'],
                    gain=100 * (episode['start_best'][0] - episode['result']['best_metrics']['score'][0]) / episode['start_best'][0]))
    grouped_completion = []
    for seed in config['seeds']:
        for ordinal in (0, config['training']['updates']):
            for position in ('pre', 'post'):
                rows = [r for r in completion['rows'] if r['seed'] == seed and r['update'] == ordinal and r['position'] == position]
                grouped_completion.append(dict(seed=seed, update=ordinal, position=position, cases=len(rows),
                    goal_reached=sum(r['goal_reached'] for r in rows), same_pose_commit=sum(r['same_pose_commit'] for r in rows),
                    any_area_improvement=sum(r['best_score'][0] < r['start_score'][0] for r in rows),
                    mean_actions=statistics.mean(r['policy_actions'] for r in rows),
                    mean_gain=statistics.mean(100 * (r['start_score'][0] - r['best_score'][0]) / r['start_score'][0] for r in rows)))
    likelihood = []
    for split in ('train', 'holdout'):
        for seed in config['seeds']:
            for ordinal in config['evaluation']['at']:
                for kind in ('pose', 'repair_all', 'commit'):
                    rows = [r for r in completion['probabilities'] if r['split'] == split and r['seed'] == seed and r['update'] == ordinal and r['kind'] == kind]
                    nll = -statistics.mean(r['log_probability'] for r in rows)
                    likelihood.append(dict(seed=seed, update=ordinal, split=split, kind=kind, nll=nll, geometric_probability=math.exp(-nll)))
    by_seed = []
    for seed in config['seeds']:
        local = [r for r in updates if r['seed'] == seed]
        events = [r for r in improvements if r['seed'] == seed]
        by_seed.append(dict(seed=seed, area_reward=sum(r['reward'] for r in local), area_commits=len(events),
            supplied_opportunity_commits=sum(r['start'] == 'repair_ready' for r in events),
            original_overlap_commits=sum(r['start'] in ('original', 'overlap') for r in events),
            actor_epochs=dict(Counter(r['actor_epochs'] for r in local)),
            bc_labels=sum(r['bc']['label_count'] for r in local),
            mean_update_seconds=statistics.mean(r['update_seconds'] for r in local),
            mean_final_kl=statistics.mean(r['final_kl'] for r in local)))
    final_events = []
    for seed in config['seeds']:
        report = read(run / f'seed-{seed}/evaluation/{config["training"]["updates"]:04d}/report.json')
        for ref in report['cases']:
            case = load(checked(ref, run))
            events = [dict(action=t['policy_action'], score=t['best_score'], elapsed_ms=t['elapsed_ms'])
                      for t in case['trace'] if t['outcome']['reward'] > 0]
            clears = [dict(action=a['policy_action'], kind=a['action']['kind'])
                      for a, b in zip(case['trace'], case['trace'][1:])
                      if a['observation']['conflicts'] and not b['observation']['conflicts']]
            final_events.append(dict(seed=seed, case=ref, area_events=events,
                                     first_overlap_cleared=clears[0] if clears else None))
    delayed_guards = []
    for row in evaluation:
        if row['elapsed_ms'] <= config['evaluation']['elapsed_ms'] + 10:
            continue
        report = read(run / f'seed-{row["seed"]}/evaluation/{row["update"]:04d}/report.json')
        ref = next(r for r in report['cases'] if r['path'].endswith(f'/{row["start"]}-{row["sample"]}.pt.gz'))
        last = load(checked(ref, run))['trace'][-1]
        require(last['outcome']['status'] == 'rejected' and not last['outcome']['charged']
                and last['best_score'] == last['observation']['best_metrics']['score']
                and last['outcome']['revision'] == last['observation']['revision'],
                'blueprint.bc-summary: delayed guard rejects without changing draft or best')
        delayed_guards.append(dict(case=ref, elapsed_ms=row['elapsed_ms'], unchanged_late_rejection=True))
    result = dict(label=label, run=str(run), coefficient=config['bc']['coefficient'], seconds=state['seconds'],
        protocol_sha256=sha(run / 'protocol.json'), series=series, evaluation=evaluation, action_curves=action_curves,
        training_by_seed=by_seed, kinds=dict(kinds), improvements=improvements, episodes=episodes,
        completion=grouped_completion, completion_rows=completion['rows'], likelihood=likelihood, final_events=final_events,
        native_audit=audit['counts'], completion_audit=completion_audit['counts'],
        delayed_guards=delayed_guards, evaluation_guards=dict(Counter(r['guard'] for r in evaluation)),
        max_evaluation_elapsed_ms=max(r['elapsed_ms'] for r in evaluation))
    return result, first


def compare(control, treatment, output):
    output.mkdir(parents=True, exist_ok=True)
    configs = [read(run / 'protocol.json')['config'] for run in (control, treatment)]
    require([c['bc']['coefficient'] for c in configs] == configs[0]['bc']['coefficients'],
            'blueprint.bc-summary: ordered registered control/treatment coefficients')
    clean = []
    for config in configs:
        config = dict(config, bc={k: v for k, v in config['bc'].items() if k != 'coefficient'})
        clean.append(config)
    require(clean[0] == clean[1], 'blueprint.bc-summary: arms differ only in BC coefficient')
    config = configs[0]
    initial = []
    for seed in config['seeds']:
        checkpoints = [load(run / f'seed-{seed}/checkpoints/0000.pt.gz') for run in (control, treatment)]
        for key in ('model', 'actor_optimizer', 'critic_optimizer', 'sampler_rng', 'shuffle_rng', 'streams'):
            require(digest(checkpoints[0][key]) == digest(checkpoints[1][key]), f'blueprint.bc-summary: identical initial {key}')
        initial.append(dict(seed=seed, model_sha256=checkpoints[0]['model_sha256']))
    arms, firsts = [], []
    for run, label in ((control, 'PPO'), (treatment, 'PPO + BC (.05)')):
        result, first = summarize_arm(run, label)
        arms.append(result)
        firsts.append(first)
    final = config['training']['updates']
    paired = []
    for row in arms[0]['evaluation']:
        if row['update'] != final:
            continue
        other = next(r for r in arms[1]['evaluation'] if all(r[k] == row[k] for k in ('seed', 'update', 'start', 'sample')))
        paired.append(dict(seed=row['seed'], start=row['start'], sample=row['sample'], control=row['best_score'], treatment=other['best_score'],
                           area='win' if other['best_score'][0] < row['best_score'][0] else 'loss' if other['best_score'][0] > row['best_score'][0] else 'tie',
                           score='win' if other['best_score'] < row['best_score'] else 'loss' if other['best_score'] > row['best_score'] else 'tie'))
    summary = dict(schema='blueprint-bc-comparison-v1', initial=initial, arms=arms, paired=paired,
        area_pairs=dict(Counter(r['area'] for r in paired)), score_pairs=dict(Counter(r['score'] for r in paired)),
        dataset=read((ROOT / config['bc']['dataset']).parent / 'summary.json'))
    fig, axes = plt.subplots(2, 2, figsize=(11, 7), constrained_layout=True)
    colors = ['#657786', '#bd4b21']
    for arm, color in zip(arms, colors, strict=True):
        for col, seed in enumerate(config['seeds']):
            rows = [r for r in arm['series'] if r['seed'] == seed]
            axes[0, col].plot([r['update'] for r in rows], [r['gain'] for r in rows], marker='o', color=color, label=arm['label'])
            axes[1, col].plot([r['update'] for r in rows], [r['pose_percent'] for r in rows], marker='o', color=color)
            axes[0, col].set_title(f'Seed {seed}')
            axes[1, col].set_xlabel('PPO update')
    axes[0, 0].set_ylabel('Mean validated area reduction (%)')
    axes[1, 0].set_ylabel('Facility movement share (%)')
    for ax in axes.flat:
        ax.grid(alpha=.2)
        ax.set_xticks(config['evaluation']['at'])
        ax.set_ylim(bottom=0)
    axes[0, 0].legend()
    fig.suptitle('Autonomous 2-second inference; same fixture, six cases per seed/checkpoint')
    fig.savefig(output / 'comparison.png', dpi=160)
    plt.close(fig)
    fig, axes = plt.subplots(1, 2, figsize=(10, 4), constrained_layout=True)
    for ax, position, title in zip(axes, ('pre', 'post'), ('From before the useful move', 'From after the useful move'), strict=True):
        for i, (arm, color) in enumerate(zip(arms, colors, strict=True)):
            rows = [r for r in arm['completion'] if r['update'] == final and r['position'] == position]
            ax.bar([j + i * .35 for j in range(len(rows))], [r['goal_reached'] for r in rows], width=.35, color=color, label=arm['label'])
        ax.set_xticks([j + .175 for j in range(len(config['seeds']))], [f'Seed {seed}' for seed in config['seeds']])
        ax.set_title(title)
        ax.set_ylim(0, 8)
        ax.set_yticks(range(9))
        ax.set_ylabel('Target area reached / 8 cases')
    axes[0].legend()
    fig.suptitle('Held-out drafts: autonomous 64-action completion, final checkpoint')
    fig.savefig(output / 'completion.png', dpi=160)
    plt.close(fig)
    original = read(ROOT / read(ROOT / config['probe_spec'])['initial'])
    if firsts[1] is not None:
        witness, event = firsts[1]
        fig, axes = plt.subplots(1, 2, figsize=(12, 5), constrained_layout=True)
        geometries = [original, witness]
        points = [(p['x'], p['y']) for g in geometries for n in g['transport_networks'] for p in n['cells']]
        points += [(p['x'] + dx, p['y'] + dy) for g in geometries for p in g['placements']
                   for dx, dy in ((0, 0), (p['width'], p['height']))]
        bounds = (min(x for x, _ in points) - 1, max(x for x, _ in points) + 1,
                  min(y for _, y in points) - 1, max(y for _, y in points) + 1)
        draw(axes[0], original, bounds)
        draw(axes[1], witness, bounds)
        axes[0].set_title('Original validated factory')
        axes[1].set_title(f'BC policy: area {event["score"][0]}, action {event["action"]}')
        fig.suptitle(f'First ordinary-start area improvement: seed {event["seed"]}, update {event["update"]}')
        fig.savefig(output / 'policy-improvement.png', dpi=160)
        plt.close(fig)
        summary['first_treatment_improvement'] = event
    summary['figures'] = [reference(p, output) for p in sorted(output.glob('*.png'))]
    write(output / 'summary.json', summary)
    print({k: v for k, v in summary.items() if k in ('initial', 'area_pairs', 'score_pairs')}, flush=True)
    for arm in arms:
        print(arm['label'], 'final:', [r for r in arm['series'] if r['update'] == final], flush=True)
        print('training:', arm['training_by_seed'], flush=True)
        print('completion:', [r for r in arm['completion'] if r['update'] == final], flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--control', type=Path, required=True)
    parser.add_argument('--treatment', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    compare(args.control.resolve(), args.treatment.resolve(), args.output.resolve())
