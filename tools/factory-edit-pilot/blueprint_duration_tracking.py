"""Attach online W&B telemetry to an existing duration run without changing training."""
import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import signal
import threading
import time

from ppo_harness_environment import load_environment, tracking_destination
from ppo_harness_state import read, sha, write
from blueprint_duration_metrics import metrics as canonical_metrics, define_metrics


def require(condition, message):
    if not condition:
        raise ValueError(f'blueprint.duration-tracking: {message}')


def checked_json(root, ref):
    path = (root / ref['path']).resolve()
    require(path.is_relative_to(root.resolve()) and sha(path) == ref['sha256'], f'evidence identity: {path}')
    return read(path)


def events(state):
    """A stable prefix even when an evaluation is published after its update."""
    require(len(state['jobs']) == 1, 'expected one learner')
    job = next(iter(state['jobs'].values()))
    limit = job['update'] - (state['status'] == 'running')
    rows = [(i, 0, ref) for i, ref in enumerate(job['records'], 1) if i <= limit]
    rows.extend((ref['update'], 1, ref) for ref in job['evaluations'] if ref['update'] == 0 or ref['update'] <= limit)
    return sorted(rows, key=lambda row: row[:2])


def metrics(root, event, actions_per_update, baseline):
    update, evaluation, ref = event
    record = checked_json(root, ref)
    return canonical_metrics(record, update, actions_per_update,
        seconds=ref['seconds'] if evaluation else record.get('elapsed_seconds'),
        initial=baseline if evaluation else None)


def trainer_alive(root):
    completion = read(root / 'completion.json')
    if completion['status'] != 'running':
        return False
    try:
        os.kill(completion['pid'], 0)
        return True
    except ProcessLookupError:
        return False


def export_pending(run, root, state, cursor, actions_per_update):
    rows = events(state)
    job = next(iter(state['jobs'].values()))
    baseline = checked_json(root, job['evaluations'][0])['rows'] if job['evaluations'] else None
    if cursor > len(rows) and state['status'] == 'running':
        # A trainer resumed from a pause can briefly expose the already exported
        # final update as the withheld running frontier. Wait for its successor.
        require(cursor <= len(events(dict(state, status='paused'))), 'remote history is ahead of durable evidence')
        return cursor
    require(0 <= cursor <= len(rows), 'remote history is ahead of durable evidence')
    for ordinal in range(cursor, len(rows)):
        run.log(metrics(root, rows[ordinal], actions_per_update, baseline), step=ordinal, commit=True)
    return len(rows)


def observe(root, destination, interval, stopped, wandb):
    protocol_hash = sha(root / 'protocol.json')
    protocol = read(root / 'protocol.json')
    require(protocol['schema'] == 'blueprint-success-duration-v2', 'unsupported protocol')
    config = protocol['config']
    identifier = hashlib.sha256(f'blueprint-duration:{protocol_hash}'.encode()).hexdigest()[:24]
    directory = root / 'wandb-telemetry'
    directory.mkdir(exist_ok=True)
    path = root / 'wandb-observer.json'
    status = dict(schema='blueprint-duration-wandb-v2', protocol_sha256=protocol_hash,
                  run_id=identifier, pid=os.getpid(), status='starting', attached_epoch=time.time())
    write(path, status)
    run = None
    try:
        run = wandb.init(**destination, id=identifier, name=f'{config["purpose"]}--seed-{config["seeds"][0]}',
            group=f'blueprint-duration-{protocol_hash[:12]}', job_type=config['purpose'], mode='online', resume='allow',
            dir=str(directory), save_code=False,
            settings=wandb.Settings(console='off', disable_code=True, x_disable_meta=True,
                x_disable_stats=True, x_disable_machine_info=True, init_timeout=20, finish_timeout=10),
            config=dict(protocol_sha256=protocol_hash, seed=config['seeds'][0], training=config['training'],
                        ppo=config['ppo'], success_coefficient=config['success']['coefficient'],
                        duration=config['duration'], evaluation=config['evaluation'],
                        telemetry='independent observer; history timestamps are export times'))
        remote_path = f'{destination["entity"]}/{destination["project"]}/{identifier}'
        remote = wandb.Api(timeout=10).run(remote_path)
        cursor = max(run.step, remote.lastHistoryStep + 1)
        define_metrics(run)
        status.update(url=run.url, status='ready', remote_confirmed_step=remote.lastHistoryStep)
        write(path, status)
        while True:
            state = read(root / 'state.json')
            require(state['protocol_sha256'] == protocol_hash, 'trainer protocol changed')
            if state['status'] == 'prepared':
                require(trainer_alive(root), 'trainer process absent before initialization')
                if stopped.wait(interval):
                    raise RuntimeError('observer stopped before trainer initialization')
                continue
            require(state['status'] in ('running', 'complete', 'paused', 'failed'), 'unsupported trainer state')
            cursor = export_pending(run, root, state, cursor,
                                    config['training']['streams'] * config['training']['chunk_actions'])
            summary = dict(source_status=state['status'], source_stop_reason=state['stop_reason'],
                training_started_at=state['started_at'], training_elapsed_seconds=state['seconds'],
                training_actions=state.get('actions', 0), positive_commits=state.get('positive_commits', 0),
                buffer_entries=state.get('buffer_entries', 0), observer_heartbeat_epoch=time.time(),
                training_complete=state['status'] == 'complete', exported_events=cursor)
            run.summary.update(summary)
            status.update(status='running', **summary)
            write(path, status)
            if state['status'] != 'running':
                break
            require(trainer_alive(root), 'trainer process is absent while durable state still says running')
            if stopped.wait(interval):
                break
        detached = state['status'] == 'running'
        reason = 'observer-detached' if detached else state['stop_reason']
        run.summary['observer_termination_reason'] = reason
        run.finish(exit_code=1 if detached or state['status'] == 'failed' else 0)
        run = None
        remote = wandb.Api(timeout=10).run(remote_path)
        require(remote.lastHistoryStep >= cursor - 1, 'final W&B history has not been acknowledged')
        status.update(status='detached' if detached else 'finished', remote_confirmed_step=remote.lastHistoryStep,
                      termination_reason=reason, finished_epoch=time.time())
        write(path, status)
    except BaseException as error:
        message = str(error)
        secret = os.environ.get('WANDB_API_KEY')
        if secret:
            message = message.replace(secret, '[redacted]')
        status.update(status='failed', diagnostic=dict(code='blueprint.duration-tracking.failed', severity='error',
                      stage='tracking', entity=str(root), message=message), finished_epoch=time.time())
        write(path, status)
        if run is not None:
            run.finish(exit_code=1)
        raise RuntimeError(message) from None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--env-file', type=Path, default=Path(__file__).resolve().parents[2] / '.env')
    parser.add_argument('--poll-seconds', type=float, default=1)
    args = parser.parse_args()
    require(args.poll_seconds > 0, '--poll-seconds must be positive')
    root = args.output.resolve()
    require((root / 'state.json').is_file(), f'missing trainer state: {root}')
    stopped = threading.Event()
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: stopped.set())
    with (root / '.wandb-observer.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise SystemExit(f'blueprint.duration-tracking: another observer owns {root}') from None
        load_environment(args.env_file)
        destination = tracking_destination(required=True)
        import wandb
        observe(root, destination, args.poll_seconds, stopped, wandb)


if __name__ == '__main__':
    main()
