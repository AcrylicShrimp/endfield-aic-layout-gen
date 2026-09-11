"""Summarize a fixed policy diagnostic and optionally snapshot later evaluations."""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

from blueprint_ppo_store import checked, read, reference, write
from blueprint_ppo_update import require


def summarize(directory, run=None):
    source = read(directory / 'summary.json')
    require(source['status'] == 'passed', 'completed policy diagnosis required')
    totals, outcomes = Counter(), defaultdict(Counter)
    for row in source['credit']:
        totals['zero_reward_batches'] += row['zero_reward']
        totals['changed_models'] += row['model_changed']
        for name, counts in row['by_outcome'].items():
            outcomes[name].update(counts)
            totals['actions'] += counts['count']
            totals['negative_rewards'] += counts['negative_rewards']
            if name.endswith('/rejected'):
                totals['rejected_actions'] += counts['count']
    evidence = dict(schema='blueprint-policy-diagnosis-compact-v1', cutoff=source['cutoff'],
        totals=dict(totals), outcomes={k: dict(v, mean_normalized_credit=v['normalized_sum'] / v['count'])
                                      for k, v in outcomes.items()},
        fixed_pose_probabilities=[dict(update=r['update'],
            training_percent=100 * r['training']['probabilities']['pose'],
            evaluation_percent=100 * r['evaluation']['probabilities']['pose']) for r in source['policy']],
        ablations=[dict(update=a['update'], results=[dict(variant=r['variant'],
            training_delta_pp=100 * (r['after']['training']['probabilities']['pose'] - a['before']['training']['probabilities']['pose']),
            evaluation_delta_pp=100 * (r['after']['evaluation']['probabilities']['pose'] - a['before']['evaluation']['probabilities']['pose']))
            for r in a['results']]) for a in source['ablations']])
    later = []
    if run is None and (directory / 'later-evaluations.json').exists():
        retained = read(directory / 'later-evaluations.json')
        require(retained['protocol_sha256'] == source['protocol_sha256'], 'same retained later evaluation protocol')
        later = retained['evaluations']
    if run is not None:
        state = read(run / 'state.json')
        require(state['protocol_sha256'] == source['protocol_sha256'], 'same live run protocol')
        for job in state['jobs'].values():
            for ref in job['evaluations']:
                if ref['update'] <= source['cutoff']:
                    continue
                rows = read(checked(ref, run))['rows']
                later.append(dict(update=ref['update'], seconds=ref['seconds'], cases=len(rows),
                    area_percent=sum(r['total_area_reduction_percent'] for r in rows) / len(rows),
                    pose_actions=sum(r['counts'].get('kind/pose', 0) for r in rows), evidence=ref))
        write(directory / 'later-evaluations.json', dict(schema='blueprint-policy-later-evaluations-v1',
            observed_at=datetime.now(timezone.utc).isoformat(), protocol_sha256=source['protocol_sha256'],
            live_status=state['status'], live_actions=state.get('actions', 0), evaluations=later))
    write(directory / 'compact.json', evidence)
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    figure, axes = plt.subplots(2, 2, figsize=(12, 8), constrained_layout=True)
    curve = source['evaluations'] + later
    axes[0, 0].plot([r['update'] for r in curve], [r['area_percent'] for r in curve], marker='.', linewidth=1)
    axes[0, 0].axhline(curve[0]['area_percent'], color='gray', linestyle='--', label='Untrained evaluation')
    axes[0, 0].axvline(source['cutoff'], color='gray', linestyle=':', label='Attribution cutoff')
    axes[0, 0].set(xlabel='Training batches', ylabel='Mean validated area reduction (%)', title='Actual 32-factory evaluation')
    axes[0, 0].legend(fontsize=8)
    for key, label in [('training_percent', 'Two training starts'), ('evaluation_percent', '32 evaluation starts')]:
        axes[0, 1].plot([r['update'] for r in evidence['fixed_pose_probabilities']],
                        [r[key] for r in evidence['fixed_pose_probabilities']], marker='.', label=label)
    axes[0, 1].set(xlabel='Checkpoint update', ylabel='Facility-pose probability (%)', title='Identical initial-state probes')
    axes[0, 1].legend(fontsize=8)
    selected = next((r for r in evidence['ablations'] if r['update'] == 9), evidence['ablations'][0])
    axes[1, 0].barh([r['variant'] for r in selected['results']], [r['evaluation_delta_pp'] for r in selected['results']])
    axes[1, 0].axvline(0, color='gray', linewidth=1)
    axes[1, 0].set(xlabel='Pose-probability change (percentage points)',
                   title=f'Update {selected["update"]}: same samples and Adam state')
    labels = ['pose/draft_accepted', 'pose/rejected', 'repair/rejected', 'repair_all/rejected', 'commit/rejected']
    axes[1, 1].barh(labels, [evidence['outcomes'][k]['mean_normalized_credit'] for k in labels])
    axes[1, 1].axvline(0, color='gray', linewidth=1)
    axes[1, 1].set(xlabel='Mean normalized PPO advantage',
                   title=f'Negative rewards: {totals["negative_rewards"]}; actions: {totals["actions"]:,}')
    figure.suptitle('Blueprint training: early movement suppression and local attribution')
    figure.savefig(directory / 'diagnosis.png', dpi=150)
    plt.close(figure)
    write(directory / 'summary-provenance.json', dict(schema='blueprint-policy-summary-provenance-v1',
        input=reference(directory / 'summary.json', directory),
        source=reference(Path(__file__), Path(__file__).resolve().parents[2])))
    print(dict(status='passed', output=str(directory), totals=dict(totals), later_evaluations=len(later)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--diagnosis', type=Path, required=True)
    parser.add_argument('--live-run', type=Path)
    args = parser.parse_args()
    summarize(args.diagnosis.resolve(), args.live_run.resolve() if args.live_run else None)


if __name__ == '__main__':
    main()
