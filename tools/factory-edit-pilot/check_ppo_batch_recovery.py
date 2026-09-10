"""Interrupt a bounded readiness learner, resume, and compare all committed states."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from compare_ppo_supply_runs import main as compare
from ppo_harness_state import read, write, checked_path, sha


def main(args):
    args.output = args.output.resolve()
    if args.output.exists():
        raise ValueError(f'fresh output required: {args.output}')
    args.output.mkdir(parents=True)
    root = args.output / 'run'
    cli = [sys.executable, str(Path(__file__).with_name('ppo_harness.py'))]
    subprocess.run(cli + ['prepare', '--config', str(args.config), '--output', str(root)], check=True)
    with (args.output / 'run.log').open('w') as log:
        child = subprocess.Popen(cli + ['run', '--run', str(root), '--duration-seconds', '300'], stdout=log, stderr=log)
        began = time.monotonic()
        try:
            while True:
                if child.poll() is not None or time.monotonic() - began > 180:
                    raise RuntimeError('readiness did not reach batch-2 learning within the stop-test bound')
                path = root / 'progress.json'
                progress = read(path) if path.exists() else {}
                if progress.get('stage') == 'learner' and progress.get('batch') == 2:
                    break
                time.sleep(.1)
            before = read(root / 'state.json')['jobs'][0]['checkpoint']
            subprocess.run(cli + ['stop', '--run', str(root)], check=True, timeout=40, stdout=log, stderr=log)
            child.wait(timeout=40)
        finally:
            if child.poll() is None:
                subprocess.run(cli + ['stop', '--run', str(root)], timeout=40, stdout=log, stderr=log)
                child.wait(timeout=40)
        stopped = read(root / 'state.json')
        job = stopped['jobs'][0]
        if stopped['status'] != 'stopped' or job['completed_batches'] != 1 or job['checkpoint'] != before:
            raise AssertionError('in-flight update changed the durable batch-1 checkpoint')
        checked_path(root, before)
        location = root / job['id'] / 'training-0002'
        attempts = list(location.glob('episode-*/attempt-0000/task.json'))
        expected = read(root / 'protocol.json')['config']['training']['attempts']
        if len(attempts) != expected:
            raise AssertionError('the stopped phase did not schedule every task')
        subprocess.run(cli + ['resume', '--run', str(root), '--duration-seconds', '300'],
                       check=True, timeout=340, stdout=log, stderr=log)
    finished = read(root / 'state.json')
    record = next(r for r in finished['jobs'][0]['records'] if r['phase'] == dict(kind='training', batch=2))
    record = read(checked_path(root, record))
    if len(record['attempts']) != expected or any(a['attempt'] != 1 for a in record['attempts']):
        raise AssertionError('resume reused an uncommitted task instead of resampling the whole phase')
    for original in attempts:
        repeated = original.parent.with_name('attempt-0001') / 'task.json'
        if sha(original) != sha(repeated):
            raise AssertionError(f'resampled task identity changed: {original}')
    compare(argparse.Namespace(before=args.reference.resolve(), after=root, output=args.output / 'comparison.json'))
    report = dict(schema='ppo-completed-batch-recovery-check-v1', status='complete', source_sha256=sha(__file__),
                  run=str(root), reference=str(args.reference.resolve()), stopped_at='batch-2-learner',
                  retained_batch=1, retained_checkpoint=before, resampled_tasks=expected,
                  all_final_states_and_epochs_exact=True)
    write(args.output / 'report.json', report)
    print(json.dumps(report), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    main(parser.parse_args())
