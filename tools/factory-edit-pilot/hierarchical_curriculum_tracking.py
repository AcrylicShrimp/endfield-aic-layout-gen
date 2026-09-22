"""Mirror committed hierarchical curriculum generations to W&B."""
import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import signal
import threading
import time

from ppo_harness_environment import load_environment, tracking_destination
from ppo_harness_state import read, write


def require(condition, message):
    if not condition:
        raise ValueError(f'hierarchical-curriculum.tracking: {message}')


def generation_path(root, generation, name):
    return root / 'generations' / f'{generation:06}' / name


def committed_metrics(root, generation):
    summary = read(generation_path(root, generation, 'summary.json'))
    require(summary['schema'] == 'aic-hierarchical-curriculum-generation-v2',
            f'unsupported generation summary: {generation}')
    require(summary['generation'] == generation, f'generation identity changed: {generation}')
    training = read(generation_path(root, generation, 'training') / 'report.json')
    require(training['schema'] == 'aic-hierarchical-generation-training-report-v3' and
            training['status'] == 'complete', f'incomplete generation training report: {generation}')
    row = {
        'generation': generation,
        'curriculum/level': summary['level'],
        'curriculum/advanced': int(summary['advanced']),
        'curriculum/recent_cases': summary['recent_training_cases'],
        'curriculum/recent_completion_rate': summary['recent_training_completion_rate'],
        'train/completed': summary['training_completed'],
        'train/cases': summary['training_cases'],
        'train/completion_rate': summary['training_completion_rate'],
        'train/mcts_samples_per_minute': summary['training_mcts_samples_per_minute'],
        'train/root_states': training['root_states'],
        'train/outer_action_samples': training['outer_action_samples'],
        'train/lower_route_samples': training['lower_route_samples'],
        'train/update_samples_per_minute': training['samples_per_minute'],
        'train/update_seconds': training['elapsed_seconds'],
        'loss/outer_initial': training['initial_outer_loss'],
        'loss/outer_final': training['final_outer_loss'],
        'loss/lower_initial': training['initial_lower_loss'],
        'loss/lower_final': training['final_lower_loss'],
        'loss/maximum_gradient_norm': training['maximum_gradient_norm'],
    }
    optional = {
        'train/mean_footprint_area': summary['training_mean_footprint_area'],
        'train/mean_transport_cells': summary['training_mean_transport_cells'],
        'train/mean_turns': summary['training_mean_turns'],
        'heldout/completed': summary['heldout_completed'],
        'heldout/cases': summary['heldout_cases'],
        'heldout/completion_rate': summary['heldout_completion_rate'],
        'heldout/mean_footprint_area': summary['heldout_mean_footprint_area'],
        'heldout/mean_transport_cells': summary['heldout_mean_transport_cells'],
        'heldout/mean_turns': summary['heldout_mean_turns'],
        'heldout/mcts_samples_per_minute': summary['heldout_mcts_samples_per_minute'],
    }
    row.update((name, value) for name, value in optional.items() if value is not None)
    return row, summary


def export_pending(run, root, state, cursor):
    completed = state['completed_generations']
    require(0 <= cursor <= completed, 'remote history is ahead of durable generations')
    latest = None
    for generation in range(cursor, completed):
        row, latest = committed_metrics(root, generation)
        run.log(row, step=generation, commit=True)
    return completed, latest


def observe(root, destination, interval, stopped, wandb):
    config = read(root / 'config.json')
    binding = read(root / 'binding.json')
    state = read(root / 'state.json')
    require(config['schema'] == 'aic-hierarchical-curriculum-v1', 'unsupported curriculum config')
    require(binding['schema'] == 'aic-hierarchical-curriculum-binding-v1', 'unsupported binding')
    require(state['schema'] == 'aic-hierarchical-curriculum-state-v3', 'unsupported trainer state')
    binding_sha256 = state['binding_sha256']
    identity = f"{binding_sha256}:{root.name}"
    identifier = hashlib.sha256(f'hierarchical-curriculum:{identity}'.encode()).hexdigest()[:24]
    telemetry = root / 'wandb-telemetry'
    telemetry.mkdir(exist_ok=True)
    status_path = root / 'wandb-observer.json'
    status = dict(schema='aic-hierarchical-curriculum-wandb-v1', run_id=identifier,
                  status='starting', pid=os.getpid(), binding_sha256=state['binding_sha256'])
    write(status_path, status)
    run = None
    try:
        run = wandb.init(**destination, id=identifier, resume='allow', mode='online',
            name=root.name, group='hierarchical-curriculum', job_type='hierarchical-mcts-training',
            dir=str(telemetry), save_code=False,
            settings=wandb.Settings(console='off', disable_code=True, x_disable_meta=True,
                x_disable_stats=True, x_disable_machine_info=True, init_timeout=30, finish_timeout=15),
            config=dict(curriculum=config, input_binding=binding,
                        telemetry='independent observer over atomically committed generations'))
        run.define_metric('generation')
        for namespace in ('curriculum/*', 'train/*', 'heldout/*', 'loss/*'):
            run.define_metric(namespace, step_metric='generation')
        remote_path = f'{destination["entity"]}/{destination["project"]}/{identifier}'
        remote = wandb.Api(timeout=15).run(remote_path)
        cursor = max(run.step, remote.lastHistoryStep + 1)
        status.update(status='running', url=run.url, exported_generations=cursor,
                      remote_confirmed_step=remote.lastHistoryStep)
        write(status_path, status)
        while True:
            state = read(root / 'state.json')
            require(state['schema'] == 'aic-hierarchical-curriculum-state-v3' and
                    state['binding_sha256'] == binding_sha256,
                    'trainer binding changed')
            cursor, latest = export_pending(run, root, state, cursor)
            report_path = root / 'report.json'
            report = read(report_path) if report_path.is_file() else None
            complete = (state['completed_generations'] == config['generations'] and
                        report is not None and report.get('status') == 'complete')
            summary = dict(source_status='complete' if complete else 'running',
                completed_generations=state['completed_generations'], configured_generations=config['generations'],
                curriculum_level=state['level'], observer_heartbeat_epoch=time.time(),
                exported_generations=cursor)
            if latest is not None:
                summary.update(latest_lower_checkpoint_sha256=latest['lower_checkpoint_sha256'],
                               latest_outer_checkpoint_sha256=latest['outer_checkpoint_sha256'])
            run.summary.update(summary)
            status.update(**summary)
            write(status_path, status)
            if complete:
                break
            if stopped.wait(interval):
                status.update(status='detached', finished_epoch=time.time())
                write(status_path, status)
                run.finish(exit_code=1)
                return
        run.finish(exit_code=0)
        run = None
        remote = wandb.Api(timeout=15).run(remote_path)
        require(remote.lastHistoryStep >= cursor - 1, 'final W&B history has not been acknowledged')
        status.update(status='finished', remote_confirmed_step=remote.lastHistoryStep,
                      finished_epoch=time.time())
        write(status_path, status)
    except BaseException as error:
        message = str(error)
        secret = os.environ.get('WANDB_API_KEY')
        if secret:
            message = message.replace(secret, '[redacted]')
        status.update(status='failed', error=message, finished_epoch=time.time())
        write(status_path, status)
        if run is not None:
            run.finish(exit_code=1)
        raise RuntimeError(message) from None
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--env-file', type=Path,
                        default=Path(__file__).resolve().parents[2] / '.env')
    parser.add_argument('--poll-seconds', type=float, default=2.0)
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
            raise SystemExit(f'hierarchical-curriculum.tracking: another observer owns {root}') from None
        load_environment(args.env_file)
        destination = tracking_destination(required=True)
        import wandb
        observe(root, destination, args.poll_seconds, stopped, wandb)


if __name__ == '__main__':
    main()
