"""Read-only CLI snapshots, local plots and W&B export of committed PPO records."""
import argparse
from pathlib import Path
import signal
import time
import uuid

from ppo_harness_state import read, write, sha, load_state, checked_path
from ppo_harness_supervisor import identity
from ppo_harness_tracking import Tracker
from ppo_harness_comparison import comparisons, tracking_jobs


def status(root):
    state = load_state(root)
    protocol = read(root / 'protocol.json')
    path = root / 'supervisor.json'
    supervisor = read(path) if path.exists() else None
    actual = state['status']
    age = time.time() - supervisor['updated_epoch'] if supervisor else None
    if actual == 'running' and (supervisor is None or age > 10 or identity(supervisor['supervisor']['pid']) != supervisor['supervisor']):
        actual = 'stale'
    progress_path = root / 'progress.json'
    progress = read(progress_path) if progress_path.exists() else None
    if progress and supervisor and progress.get('session') != supervisor['session']:
        progress = None
    tracking_path = root / 'tracking.json'
    tracking = read(tracking_path) if tracking_path.exists() else None
    observer = dict(exit_code=supervisor.get('observer_exit_code'), finalization=supervisor.get('observer_finalization'),
                    child_exit_codes=supervisor.get('child_exit_codes')) if supervisor else None
    tracking_health = 'disabled' if protocol['config']['tracking']['backend'] == 'none' else 'unavailable' if not tracking else tracking['status']
    if tracking and tracking_health not in ('complete', 'disabled', 'failed'):
        observer_exit = observer['exit_code'] if observer else None
        age_tracking = time.time() - tracking.get('updated_epoch', 0)
        if observer_exit not in (None, 0):
            tracking_health = 'failed'
        elif actual in ('complete', 'stopped', 'failed', 'stale'):
            tracking_health = 'pending'
        elif age_tracking > 60 or observer_exit == 0:
            tracking_health = 'stale'
    return dict(schema='ppo-harness-status-v1', status=actual, durable_status=state['status'], measurement_age_seconds=age,
                completed_jobs=state['cursor'], planned_jobs=len(protocol['jobs']),
                completed_batches=sum(j['completed_batches'] for j in state['jobs']),
                admitted_training_actions=sum(j['selected_training_actions'] for j in state['jobs']),
                elapsed_seconds=supervisor['elapsed_seconds'] if supervisor else state['elapsed_seconds'],
                remaining_seconds=max(0, protocol['config']['limits']['run_seconds'] -
                                      (supervisor['elapsed_seconds'] if supervisor else state['elapsed_seconds'])),
                resources=supervisor.get('resources') if supervisor else None, progress=progress, tracking=tracking,
                tracking_health=tracking_health, observer=observer,
                diagnostic=state.get('diagnostic') or (supervisor.get('diagnostic') if supervisor else None))


def snapshot(root, cache=None):
    cache = {} if cache is None else cache
    state = load_state(root)
    jobs = []
    for job in state['jobs']:
        records = []
        for reference in job['records']:
            key = (reference['path'], reference['sha256'])
            if key not in cache:
                cache[key] = read(checked_path(root, reference))
            records.append(cache[key])
        jobs.append(dict(job, records=records))
    return dict(status=state['status'], protocol_sha256=state['protocol_sha256'], jobs=jobs)


def plot(root, output, value=None):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    value = snapshot(root) if value is None else value
    matched = comparisons(value, read(root / 'protocol.json'))
    output.mkdir(parents=True, exist_ok=True)
    figure, axes = plt.subplots(3, 3, figsize=(16, 12), layout='constrained')
    candidates = list(dict.fromkeys(j['candidate'] for j in value['jobs']))
    colors = {c: plt.get_cmap('tab20')(i % 20) for i, c in enumerate(candidates)}
    for job in value['jobs']:
        independent = [r for r in job['records'] if r['phase']['kind'] == 'evaluation']
        evaluations = matched.get(job['id'], [])
        training = [r for r in job['records'] if r['phase']['kind'] == 'training']
        color = colors[job['candidate']]
        style = '-' if job['seed'] == 17 else '--'
        label = f"{job['candidate']} / {job['seed']}"
        for column, x in enumerate(([r['phase']['batch'] for r in evaluations],
                                    [r['selected_training_actions'] for r in evaluations],
                                    [r['elapsed_seconds'] / 60 for r in evaluations])):
            axes[0, column].plot(x, [r['quality']['mean_area_reduction_percent'] for r in evaluations],
                                  marker='o', linestyle=style, color=color, label=label, markersize=3)
        for column, key in enumerate(('approximate_kl', 'explained_variance', 'entropy')):
            axes[1, column].plot([r['selected_training_actions'] for r in training],
                                [r['ppo']['epochs'][-1][key] for r in training], linestyle=style, color=color)
        axes[2, 0].plot([r['phase']['batch'] for r in evaluations],
                        [r['paired_to_initial']['paired_delta_pp'] for r in evaluations], linestyle=style, color=color)
        axes[2, 1].plot([r['phase']['batch'] for r in independent],
                        [r['quality']['accepted'] / r['quality']['attempted'] for r in independent], linestyle=style, color=color)
        axes[2, 2].plot([r['phase']['batch'] for r in independent],
                        [r['quality']['mean_area_reduction_percent'] for r in independent], linestyle=style, color=color)
    for column, xlabel in enumerate(('Completed training batches', 'Admitted training actions', 'Elapsed job minutes')):
        axes[0, column].set(xlabel=xlabel, ylabel='Shared-cohort held-out area reduction (%)')
    for column, title in enumerate(('Sampled KL', 'Critic explained variance', 'Policy entropy')):
        axes[1, column].set(xlabel='Admitted training actions', title=title)
    for column, title in enumerate(('Shared-cohort change from initial policy (pp)', 'Independent evaluation admission fraction',
                                    'Independent admitted mean (%) — provisional')):
        axes[2, column].set(xlabel='Completed training batches', title=title)
    for axis in axes.flat:
        axis.grid(alpha=.2)
    if candidates:
        axes[0, 2].legend(fontsize=5, frameon=False, ncol=2)
    figure.suptitle(f"PPO input contracts · {value['status']} · shared curves appear after all candidates for a seed complete")
    for extension in ('png', 'svg'):
        temporary = output / f'outcomes.tmp.{extension}'
        figure.savefig(temporary, dpi=140)
        temporary.replace(output / f'outcomes.{extension}')
    plt.close(figure)
    # The small plot snapshot contains metrics rather than copied raw trajectories.
    compact = dict(status=value['status'], protocol_sha256=value['protocol_sha256'], jobs=[dict(
        id=j['id'], candidate=j['candidate'], seed=j['seed'], status=j['status'],
        points=[dict(phase=r['phase'], actions=r['selected_training_actions'], seconds=r['elapsed_seconds'],
                     quality=r['quality'], paired=r.get('paired_to_initial'), ppo=r.get('ppo')) for r in j['records']])
        for j in value['jobs']], shared_comparisons=matched)
    write(output / 'snapshot.json', compact)


def watch(root, session):
    protocol = read(root / 'protocol.json')
    tracker = Tracker(root, protocol, sha(root / 'protocol.json'), session)
    stopped, cache, previous, retry_at = [], {}, None, 0.
    def stop(signum, _):
        stopped.append(signum)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        while not stopped:
            value = snapshot(root, cache)
            if status(root)['status'] == 'stale':
                value['status'] = 'stale'
            terminal = value['status'] in ('complete', 'failed', 'stopped', 'stale')
            signature = (value['status'], tuple((j['id'], len(j['records']), j['status']) for j in value['jobs']))
            if signature != previous:
                try:
                    plot(root, root / 'view', value)
                except Exception as error:
                    write(root / 'plot-error.json', dict(code='ppo-harness.plot-failed', severity='error', stage='monitor',
                                                       entity=str(root), message=str(error)))
                previous = signature
            if terminal or time.monotonic() >= retry_at:
                try:
                    tracker.update(tracking_jobs(value, protocol))
                    tracker.value['status'] = 'disabled' if not tracker.enabled else 'complete' if terminal else 'running'
                    tracker.publish()
                except Exception as error:
                    tracker.failed(error)
                    retry_at = time.monotonic() + 30
                    if terminal:
                        raise
            if terminal:
                break
            time.sleep(1)
    finally:
        tracker.finish(complete=False)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    observer = commands.add_parser('watch')
    observer.add_argument('--run', type=Path, required=True)
    observer.add_argument('--session', default=None)
    args = parser.parse_args()
    watch(args.run.resolve(), args.session or uuid.uuid4().hex)
