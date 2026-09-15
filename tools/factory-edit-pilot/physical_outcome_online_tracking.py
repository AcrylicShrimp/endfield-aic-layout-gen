"""Mirror committed online encoder metrics to W&B without blocking training."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import signal
import threading
import time

from ppo_harness_environment import load_environment, tracking_destination


def read(path):
    with Path(path).open() as source:
        return json.load(source)


def write(path, value):
    pending = Path(path).with_suffix('.pending')
    with pending.open('w') as target:
        json.dump(value, target, indent=2, sort_keys=True)
        target.write('\n')
        target.flush()
        os.fsync(target.fileno())
    pending.replace(path)


def rows(path):
    if not path.is_file():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def observe(root, destination, interval, stopped, wandb):
    state = read(root / 'state.json')
    if state['schema'] != 'aic-physical-outcome-online-state-v1':
        raise ValueError('physical-outcome-online.tracking: unsupported trainer state')
    identity = f"{state['config_sha256']}:{state['catalog_sha256']}:{root.name}"
    identifier = hashlib.sha256(f'physical-outcome-online:{identity}'.encode()).hexdigest()[:24]
    telemetry = root / 'wandb-telemetry'
    telemetry.mkdir(exist_ok=True)
    status_path = root / 'wandb-observer.json'
    status = dict(schema='aic-physical-outcome-online-wandb-v1', run_id=identifier,
                  status='starting', pid=os.getpid())
    write(status_path, status)
    run = None
    try:
        config = read(root / 'config.json')
        run = wandb.init(**destination, id=identifier, resume='allow', mode='online',
            name=root.name, group='physical-outcome-online-encoder',
            job_type='encoder-pretraining', dir=str(telemetry), save_code=False,
            settings=wandb.Settings(console='off', disable_code=True, x_disable_meta=True,
                x_disable_stats=True, x_disable_machine_info=True, init_timeout=30, finish_timeout=15),
            config=dict(config, data_lineage='100% live-generated; no retained dataset or replay',
                        config_sha256=state['config_sha256'], catalog_sha256=state['catalog_sha256']))
        run.define_metric('update')
        for name in ('elapsed_seconds', 'training_roots', 'training_rows', 'train_loss',
                     'gradient_norm', 'development_loss', 'rows_per_second'):
            run.define_metric(name, step_metric='update')
        cursor = run.step
        status.update(status='running', url=run.url, exported_records=cursor)
        write(status_path, status)
        while True:
            state = read(root / 'state.json')
            metrics = rows(root / 'metrics.jsonl')
            if cursor > len(metrics):
                raise ValueError('physical-outcome-online.tracking: remote history exceeds durable metrics')
            for metric in metrics[cursor:]:
                metric.pop('schema', None)
                metric.pop('labels', None)
                run.log(metric, commit=True)
                cursor += 1
            run.summary.update(dict(source_status=state['status'], stop_reason=state['stop_reason'],
                completed_updates=state['update'], training_roots=state['training_roots'],
                training_rows=state['training_rows'], observer_heartbeat_epoch=time.time()))
            status.update(exported_records=cursor, source_status=state['status'])
            write(status_path, status)
            if state['status'] != 'running':
                break
            try:
                os.kill(state['pid'], 0)
            except ProcessLookupError:
                raise RuntimeError('physical-outcome-online.tracking: trainer exited while state remained running')
            if stopped.wait(interval):
                status['status'] = 'detached'
                write(status_path, status)
                run.finish(exit_code=1)
                return
        run.finish(exit_code=0 if state['status'] == 'complete' else 1)
        run = None
        status.update(status='finished', finished_epoch=time.time())
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
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--env-file', type=Path,
                        default=Path(__file__).resolve().parents[2] / '.env')
    parser.add_argument('--poll-seconds', type=float, default=2.0)
    args = parser.parse_args()
    root = args.output.resolve()
    stopped = threading.Event()
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: stopped.set())
    with (root / '.wandb-observer.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        load_environment(args.env_file)
        destination = tracking_destination(required=True)
        import wandb
        observe(root, destination, args.poll_seconds, stopped, wandb)


if __name__ == '__main__':
    main()
