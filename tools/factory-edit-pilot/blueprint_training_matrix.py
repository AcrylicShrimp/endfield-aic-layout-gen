"""Supervise a source-bound matrix of independently audited blueprint learners."""
from datetime import datetime, timezone
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from blueprint_duration_store import exclusive_run
from blueprint_ppo_store import ROOT, checked, read, reference, sha, write
from blueprint_ppo_update import require


def completed_arm(output, arm, expected_updates):
    root = output / arm['directory']
    completion, state = read(root / 'completion.json'), read(root / 'state.json')
    require(completion['status'] == state['status'] == 'complete', f'arm did not complete: {root}')
    require(state['jobs'][str(arm['seed'])]['update'] == expected_updates, f'arm stopped before matched budget: {root}')
    audit = read(checked(completion['tail_audit'], root))
    require(audit['status'] == 'passed', f'arm tail audit did not pass: {root}')
    observer = read(root / 'wandb-observer.json')
    require(observer['status'] == 'finished', f'arm tracking did not finish: {root}')
    return dict(completion=reference(root / 'completion.json', output),
                state=reference(root / 'state.json', output), audit=reference(root / 'audit.json', output),
                observer=reference(root / 'wandb-observer.json', output), url=observer['url'])


def run_matrix(output, env_file, cpu_slots):
    with exclusive_run(output):
        protocol, state = read(output / 'matrix.json'), read(output / 'state.json')
        require(state['matrix_sha256'] == sha(output / 'matrix.json'), 'matrix protocol binding')
        require(state['status'] in ('prepared', 'paused'), f'matrix is not launchable: {state["status"]}')
        for path, expected in protocol['sources'].items():
            require(sha(ROOT / path) == expected, f'matrix source changed: {path}')
        require(cpu_slots and len(cpu_slots) == len(set(cpu_slots)) <= protocol['spec']['max_parallel'],
                'explicit distinct CPU slots within matrix parallel bound')
        require(hasattr(os, 'sched_getaffinity') and set(cpu_slots) <= os.sched_getaffinity(0),
                f'CPU slots unavailable on this host: {cpu_slots}')
        for arm in protocol['arms']:
            checked(arm['protocol'], output)
        active, stopped = {}, [False]
        pending = [a for a in protocol['arms'] if state['arms'][a['name']]['status'] != 'complete']
        require(all(state['arms'][a['name']]['status'] in ('queued', 'paused') for a in pending),
                'failed/interrupted arms require an explicit new matrix')
        previous = {s: signal.signal(s, lambda *_: stopped.__setitem__(0, True)) for s in (signal.SIGTERM, signal.SIGINT)}
        state.update(status='running', pid=os.getpid(), cpu_slots=cpu_slots)
        state.setdefault('started_at', datetime.now(timezone.utc).isoformat())
        try:
            while pending or active:
                occupied = {entry['cpu'] for entry in active.values()}
                for cpu in cpu_slots:
                    if stopped[0] or not pending or cpu in occupied:
                        continue
                    arm = pending.pop(0)
                    directory = output / arm['directory']
                    command = ['taskset', '--cpu-list', str(cpu), sys.executable,
                        str(Path(__file__).with_name('blueprint_duration_job.py')), '--output', str(directory),
                        '--env-file', str(env_file)]
                    with (directory / 'job.log').open('a') as log:
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
                    active[arm['name']] = dict(arm=arm, process=process, cpu=cpu, stop_sent=False)
                    state['arms'][arm['name']].update(status='running', pid=process.pid, cpu=cpu,
                        started_at=datetime.now(timezone.utc).isoformat())
                    print(dict(event='started', arm=arm['name'], pid=process.pid, cpu=cpu), flush=True)
                for name, entry in list(active.items()):
                    process, arm = entry['process'], entry['arm']
                    if stopped[0] and not entry['stop_sent'] and process.poll() is None:
                        child_state = read(output / arm['directory'] / 'state.json')
                        if child_state['status'] == 'running':
                            process.terminate()
                            entry['stop_sent'] = True
                        elif child_state['status'] != 'prepared':
                            entry['stop_sent'] = True
                    code = process.poll()
                    if code is None:
                        continue
                    result = state['arms'][name]
                    try:
                        require(code == 0, f'child process exited {code}: {name}')
                        if stopped[0] and read(output / arm['directory'] / 'state.json')['status'] == 'paused':
                            result.update(status='paused', exit_code=code)
                        else:
                            result.update(status='complete', exit_code=code,
                                **completed_arm(output, arm, protocol['spec']['updates']))
                    except (OSError, ValueError) as error:
                        result.update(status='failed', exit_code=code, diagnostic=dict(code='blueprint.matrix.arm-failed',
                            severity='error', stage='training-matrix', entity=name, message=str(error)))
                    result['finished_at'] = datetime.now(timezone.utc).isoformat()
                    del active[name]
                    print(dict(event='finished', arm=name, status=result['status']), flush=True)
                write(output / 'state.json', state)
                if stopped[0] and not active:
                    break
                if pending or active:
                    time.sleep(1)
            state.update(status='paused' if stopped[0] else
                'complete' if all(a['status'] == 'complete' for a in state['arms'].values()) else 'failed',
                finished_at=datetime.now(timezone.utc).isoformat())
            write(output / 'state.json', state)
            require(state['status'] != 'failed', f'matrix has failed arms: {output / "state.json"}')
        finally:
            for signum, handler in previous.items():
                signal.signal(signum, handler)
            for entry in active.values():
                if entry['process'].poll() is None:
                    entry['process'].terminate()
