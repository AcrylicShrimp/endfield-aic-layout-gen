"""Compare audited self-imitation arms and distinguish sampling from positive weight."""
import argparse
from collections import Counter
from pathlib import Path
import statistics

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

from blueprint_ppo_store import ROOT, checked, load, read, reference, sha, write
from blueprint_ppo_update import digest, require
from plot_blueprint_edit import draw


def mean(values):
    values = list(values)
    return statistics.mean(values) if values else None


def arm_summary(run, label):
    protocol, state, audit = [read(run / f'{name}.json') for name in ('protocol', 'state', 'audit')]
    require(state['status'] == 'complete' and audit['status'] == 'passed', 'blueprint.success-summary: completed audited run')
    require(state['protocol_sha256'] == audit['protocol_sha256'] == sha(run / 'protocol.json'),
            'blueprint.success-summary: protocol binding')
    config, evaluation = protocol['config'], audit['evaluation']
    series, action_curves, updates, events, episodes, entries = [], [], [], [], [], []
    first = None
    for seed in config['seeds']:
        for ordinal in config['evaluation']['at']:
            rows = [r for r in evaluation if r['seed'] == seed and r['update'] == ordinal]
            counts = Counter()
            for row in rows:
                counts.update(row['counts'])
            actions = sum(r['policy_actions'] for r in rows)
            series.append(dict(seed=seed, update=ordinal, gain=mean(r['policy_area_reduction_percent'] for r in rows),
                pose_percent=100 * counts['kind/pose'] / actions, actions=actions,
                by_start={start: mean(r['policy_area_reduction_percent'] for r in rows if r['start'] == start)
                          for start in config['evaluation']['starts']}))
            for milestone in config['evaluation']['action_milestones']:
                gains = [100 * (r['start_score'][0] - p['best_score'][0]) / r['start_score'][0]
                         for r in rows for p in r['action_milestones'] if p['policy_action'] == milestone]
                action_curves.append(dict(seed=seed, update=ordinal, actions=milestone, cases=len(gains), mean_gain=mean(gains)))
        for ref in state['jobs'][str(seed)]['records']:
            record = read(checked(ref, run))
            chunk = load(checked(record['chunk'], run))
            ordinal, replay = record['update'], record['ppo']['success']
            selected = [identity for group in replay['selected_ids'] for identity in group]
            bank = chunk['buffer']['before']['entries']
            ages = [ordinal - row['reference'][0] for identity in selected for row in bank[identity]['labels']]
            require(all(age > 0 for age in ages), 'blueprint.success-summary: strictly older replay labels')
            diagnostics = replay.get('diagnostics', [])
            labels = replay['label_count']
            updates.append(dict(seed=seed, update=ordinal, reward=record['area_reward'],
                actor_epochs=record['ppo']['actor_epochs'], final_kl=record['ppo']['epochs'][-1]['approximate_kl'],
                update_seconds=record['ppo']['seconds'], collection_seconds=record['collection_seconds'],
                entries=record['buffer']['entries'], archived=record['buffer']['positive_commits'],
                bank_labels=record['buffer']['labels'], duplicates=record['buffer']['duplicates'],
                sequence_draws=len(selected), distinct_sampled=len(set(selected)), label_draws=labels,
                positive_labels=replay['positive_labels'], weighted_loss=replay['weighted_loss'],
                positive_fraction=replay['positive_labels'] / labels if labels else None,
                mean_weight=sum(d['mean_weight'] * d['labels'] for d in diagnostics) / labels if labels else None,
                max_weight=max((d['max_weight'] for d in diagnostics), default=None),
                mean_return=sum(d['mean_return'] * d['labels'] for d in diagnostics) / labels if labels else None,
                mean_value=sum(d['mean_value'] * d['labels'] for d in diagnostics) / labels if labels else None,
                source_age_min=min(ages, default=None), source_age_mean=mean(ages), source_age_max=max(ages, default=None)))
            for event in chunk['buffer']['events']:
                if event['kind'] != 'archived':
                    continue
                commit_update, sample_index = event['commit']
                require(commit_update == ordinal, 'blueprint.success-summary: archive at discovery')
                sample = chunk['samples'][sample_index]
                detail = dict(seed=seed, update=ordinal, stream=sample['stream'], episode=sample['episode'],
                    start=sample['start'], action=sample['policy_action'], reward=sample['reward'],
                    score=sample['outcome']['best_metrics']['score'], segment_actions=len(event['references']),
                    label_stages=dict(Counter(r['stage'] for r in event['labels'])), chunk=record['chunk'])
                events.append(detail)
                if first is None:
                    first = (sample['validated_witness'], detail)
            for episode in chunk['completed']:
                episodes.append(dict(seed=seed, stream=episode['stream'], episode=episode['episode'], start=episode['start'],
                    start_score=episode['start_best'], best_score=episode['result']['best_metrics']['score'],
                    gain=100 * (episode['start_best'][0] - episode['result']['best_metrics']['score'][0]) / episode['start_best'][0]))
        checkpoint = load(run / f'seed-{seed}/checkpoints/{config["training"]["updates"]:04d}.pt.gz')
        require(not checkpoint['buffer']['active'], 'blueprint.success-summary: all episodes finished')
        for entry in checkpoint['buffer']['entries'].values():
            entries.append(dict(seed=seed, id=entry['id'], admitted_update=entry['admitted_update'],
                occurrences=entry['occurrences'], sequence_draws=entry['uses'],
                labels=len(entry['labels']), stages=dict(Counter(r['stage'] for r in entry['labels'])),
                returns=sorted({r['return_to_terminal'] for r in entry['labels']}),
                sources=[r['reference'] for r in entry['labels']], commit=entry['segment']['commit']))
    delayed = []
    for row in evaluation:
        if row['elapsed_ms'] <= config['evaluation']['elapsed_ms'] + 10:
            continue
        report = read(run / f'seed-{row["seed"]}/evaluation/{row["update"]:04d}/report.json')
        ref = next(r for r in report['cases'] if r['path'].endswith(f'/{row["start"]}-{row["sample"]}.pt.gz'))
        last = load(checked(ref, run))['trace'][-1]
        require(last['outcome']['status'] == 'rejected' and not last['outcome']['charged']
                and last['best_score'] == last['observation']['best_metrics']['score']
                and last['outcome']['revision'] == last['observation']['revision'],
                'blueprint.success-summary: delayed native guard preserves draft and best')
        delayed.append(dict(case=ref, elapsed_ms=row['elapsed_ms'], unchanged_late_rejection=True))
    by_seed = []
    for seed in config['seeds']:
        local = [r for r in updates if r['seed'] == seed]
        successes = [e for e in events if e['seed'] == seed]
        bank = [e for e in entries if e['seed'] == seed]
        draws = sum(r['label_draws'] for r in local)
        require(sum(e['occurrences'] for e in bank) == len(successes),
                'blueprint.success-summary: every archive retained through final admission or deduplication')
        require(sum(e['sequence_draws'] for e in bank) == sum(r['sequence_draws'] for r in local),
                'blueprint.success-summary: durable usage matches all learner draws')
        first_update = lambda predicate: next((r['update'] for r in local if predicate(r)), None)
        by_seed.append(dict(seed=seed, positive_commits=len(successes), reward=sum(r['reward'] for r in local),
            first_success_update=successes[0]['update'] if successes else None,
            first_admission_update=min((r['admitted_update'] for r in bank), default=None),
            first_sampled_update=first_update(lambda r: r['label_draws']),
            first_positive_weight_update=first_update(lambda r: r['positive_labels']),
            entries=len(bank), duplicate_occurrences=sum(e['occurrences'] - 1 for e in bank),
            sequence_draws=sum(r['sequence_draws'] for r in local), label_draws=draws,
            positive_labels=sum(r['positive_labels'] for r in local),
            positive_fraction=sum(r['positive_labels'] for r in local) / draws if draws else None,
            updates_with_positive_weight=sum(r['positive_labels'] > 0 for r in local),
            updates_fully_gated=sum(r['label_draws'] > 0 and r['positive_labels'] == 0 for r in local),
            oldest_replay_source=max((r['source_age_max'] for r in local if r['source_age_max'] is not None), default=None),
            actor_epochs=dict(Counter(r['actor_epochs'] for r in local)),
            mean_update_seconds=mean(r['update_seconds'] for r in local), mean_final_kl=mean(r['final_kl'] for r in local)))
    require(sum(r['positive_labels'] for r in updates) == audit['counts']['positive_weight_labels']
            and sum(r['label_draws'] for r in updates) == audit['counts']['sampled_success_labels'],
            'blueprint.success-summary: imitation totals match exact replay audit')
    return dict(label=label, run=str(run), coefficient=config['success']['coefficient'], seconds=state['seconds'],
        protocol_sha256=sha(run / 'protocol.json'), evaluation=evaluation, series=series, action_curves=action_curves,
        updates=updates, training_by_seed=by_seed, improvements=events, episodes=episodes, entries=entries,
        native_audit=audit['counts'], evaluation_guards=dict(Counter(r['guard'] for r in evaluation)),
        max_evaluation_elapsed_ms=max(r['elapsed_ms'] for r in evaluation), delayed_guards=delayed), first


def compare(control, treatment, output):
    output.mkdir(parents=True, exist_ok=True)
    protocols = [read(run / 'protocol.json') for run in (control, treatment)]
    configs = [p['config'] for p in protocols]
    require([c['success']['coefficient'] for c in configs] == configs[0]['success']['coefficients'],
            'blueprint.success-summary: registered coefficients in order')
    normalized = [dict(c, success={k: v for k, v in c['success'].items() if k != 'coefficient'}) for c in configs]
    require(normalized[0] == normalized[1], 'blueprint.success-summary: only coefficient differs')
    require(all(protocols[0][key] == protocols[1][key] for key in ('sources', 'native_sha256', 'native_build', 'runtime')),
            'blueprint.success-summary: same runtime and native binary')
    config, initial = configs[0], []
    for seed in config['seeds']:
        checkpoints = [load(run / f'seed-{seed}/checkpoints/0000.pt.gz') for run in (control, treatment)]
        for key in ('model', 'actor_optimizer', 'critic_optimizer', 'sampler_rng', 'shuffle_rng', 'streams'):
            require(digest(checkpoints[0]['learner'][key]) == digest(checkpoints[1]['learner'][key]),
                    f'blueprint.success-summary: identical initial {key}')
        require(checkpoints[0]['buffer'] == checkpoints[1]['buffer'] and not checkpoints[0]['buffer']['entries'],
                'blueprint.success-summary: matching cold empty buffers')
        initial.append(dict(seed=seed, model_sha256=checkpoints[0]['learner']['model_sha256']))
    arms, firsts = [], []
    for run, label in ((control, 'PPO'), (treatment, 'PPO + self-imitation (.05)')):
        arm, first = arm_summary(run, label)
        arms.append(arm)
        firsts.append(first)
    paired, final = [], config['training']['updates']
    for row in arms[0]['evaluation']:
        if row['update'] != final:
            continue
        other = next(r for r in arms[1]['evaluation'] if all(r[k] == row[k] for k in ('seed', 'update', 'start', 'sample')))
        paired.append(dict(seed=row['seed'], start=row['start'], sample=row['sample'], control=row['best_score'], treatment=other['best_score'],
            area='win' if other['best_score'][0] < row['best_score'][0] else 'loss' if other['best_score'][0] > row['best_score'][0] else 'tie',
            score='win' if other['best_score'] < row['best_score'] else 'loss' if other['best_score'] > row['best_score'] else 'tie'))
    summary = dict(schema='blueprint-success-comparison-v1', initial=initial, arms=arms, paired=paired,
        area_pairs=dict(Counter(r['area'] for r in paired)), score_pairs=dict(Counter(r['score'] for r in paired)))
    colors = ['#657786', '#bd4b21']
    fig, axes = plt.subplots(2, 2, figsize=(11, 7), constrained_layout=True)
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
    fig.suptitle('Autonomous 2-second inference; original and overlap starts only')
    fig.savefig(output / 'comparison.png', dpi=160)
    plt.close(fig)
    fig, axes = plt.subplots(3, 2, figsize=(11, 9), constrained_layout=True)
    for col, seed in enumerate(config['seeds']):
        for arm, color in zip(arms, colors, strict=True):
            rows = [r for r in arm['updates'] if r['seed'] == seed]
            axes[0, col].step([r['update'] for r in rows], [r['entries'] for r in rows], where='post', color=color, label=arm['label'])
        rows = [r for r in arms[1]['updates'] if r['seed'] == seed and r['label_draws']]
        axes[1, col].plot([r['update'] for r in rows], [100 * r['positive_fraction'] for r in rows], color=colors[1])
        axes[2, col].plot([r['update'] for r in rows], [r['mean_return'] for r in rows], label='Sampled terminal return', color='#177b66')
        axes[2, col].plot([r['update'] for r in rows], [r['mean_value'] for r in rows], label='Current critic value', color='#7b4cab')
        axes[0, col].set_title(f'Seed {seed}')
        axes[1, col].set_ylim(0, 101)
        axes[2, col].set_xlabel('PPO update')
    axes[0, 0].set_ylabel('Unique completed sequences')
    axes[1, 0].set_ylabel('Positive-weight label draws (%)')
    axes[2, 0].set_ylabel('Mean value / return (reward units)')
    axes[0, 0].legend()
    axes[2, 0].legend()
    for ax in axes.flat:
        ax.grid(alpha=.2)
        ax.set_xlim(0, final)
    fig.suptitle('Success retention and actual imitation gating; draws may have zero weight')
    fig.savefig(output / 'buffer.png', dpi=160)
    plt.close(fig)
    if firsts[1] is not None:
        witness, event = firsts[1]
        original = read(ROOT / read(ROOT / config['probe_spec'])['initial'])
        geometries = [original, witness]
        points = [(p['x'], p['y']) for g in geometries for n in g['transport_networks'] for p in n['cells']]
        points += [(p['x'] + dx, p['y'] + dy) for g in geometries for p in g['placements']
                   for dx, dy in ((0, 0), (p['width'], p['height']))]
        bounds = (min(x for x, _ in points) - 1, max(x for x, _ in points) + 1,
                  min(y for _, y in points) - 1, max(y for _, y in points) + 1)
        fig, axes = plt.subplots(1, 2, figsize=(12, 5), constrained_layout=True)
        draw(axes[0], original, bounds)
        draw(axes[1], witness, bounds)
        axes[0].set_title('Original validated factory')
        axes[1].set_title(f'Policy success: area {event["score"][0]}, action {event["action"]}')
        fig.suptitle(f'First treatment training improvement: seed {event["seed"]}, update {event["update"]}')
        fig.savefig(output / 'policy-improvement.png', dpi=160)
        plt.close(fig)
        summary['first_treatment_improvement'] = event
    summary['figures'] = [reference(p, output) for p in sorted(output.glob('*.png'))]
    write(output / 'summary.json', summary)
    print({k: v for k, v in summary.items() if k in ('initial', 'area_pairs', 'score_pairs')}, flush=True)
    for arm in arms:
        print(arm['label'], arm['training_by_seed'], flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--control', type=Path, required=True)
    parser.add_argument('--treatment', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    compare(args.control.resolve(), args.treatment.resolve(), args.output.resolve())


if __name__ == '__main__':
    main()
