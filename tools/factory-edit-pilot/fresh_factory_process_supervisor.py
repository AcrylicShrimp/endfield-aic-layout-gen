"""Linux supervisor for a detached, duration-bounded fresh-factory continuation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def birth(pid):
    try:
        return (Path('/proc') / str(pid) / 'stat').read_text().rpartition(')')[2].split()[19]
    except (FileNotFoundError, ProcessLookupError):
        return None


def processes():
    result = subprocess.run(['ps', '-eo', 'pid=,ppid=,pgid='], capture_output=True, text=True, check=True)
    return [tuple(map(int, line.split())) for line in result.stdout.splitlines() if line.strip()]


def capture_tree(root_pid, identities):
    if root_pid in identities and birth(root_pid) != identities[root_pid][0]:
        return
    rows = processes()
    descendants = {root_pid}
    while True:
        expanded = descendants | {pid for pid, parent, _ in rows if parent in descendants}
        if expanded == descendants:
            break
        descendants = expanded
    for pid, _, group in rows:
        if pid in descendants:
            started = birth(pid)
            if started is not None:
                identities[pid] = (started, group)


def live_owned(identities):
    return {pid: group for pid, (started, group) in identities.items() if birth(pid) == started}


def stop_tree(child, identities, grace=15):
    capture_tree(child.pid, identities)
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            pass
    # Worker processes own separate sessions. Match their recorded Linux birth
    # identities before signalling any group, including groups of orphan workers.
    for signum in (signal.SIGTERM, signal.SIGKILL):
        for group in set(live_owned(identities).values()):
            try:
                os.killpg(group, signum)
            except ProcessLookupError:
                pass
        try:
            child.wait(timeout=max(.1, grace))
        except subprocess.TimeoutExpired:
            pass
        time.sleep(.1)
    # Linux zombies have finished execution and cannot consume worker resources.
    remaining = []
    for pid in live_owned(identities):
        try:
            state = (Path('/proc') / str(pid) / 'stat').read_text().rpartition(')')[2].split()[0]
            if state != 'Z':
                remaining.append(pid)
        except FileNotFoundError:
            pass
    if remaining:
        raise RuntimeError(f'fresh-supervisor.surviving-processes: {remaining}')


def write(path, record):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(record, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


def run(args):
    if not Path('/proc/self/stat').exists():
        raise RuntimeError('fresh-supervisor.linux-required')
    screen = args.screen.resolve()
    protocol = json.loads((screen / 'protocol.json').read_text())
    expected = protocol['sources'][str(Path(__file__).resolve())]
    if hashlib.sha256(Path(__file__).read_bytes()).hexdigest() != expected:
        raise ValueError('fresh-supervisor.changed-source')
    if (screen / 'supervision.json').exists():
        raise ValueError(f'fresh-supervisor.existing-run: {screen}')
    limits = protocol['continuation']
    seconds = limits['training_seconds'] + limits['finishing_seconds'] + protocol['spec']['limits']['startup_seconds'] + 30
    identities, child = {}, None
    began = time.monotonic()
    record = dict(status='starting', supervisor_pid=os.getpid(), hard_seconds=seconds, started_epoch=time.time())
    try:
        with open(screen / 'continuation.log', 'w') as log:
            child = subprocess.Popen([sys.executable, str(Path(__file__).with_name('continue_fresh_factory_learning.py')),
                'run', '--screen', str(screen)], stdin=subprocess.DEVNULL, stdout=log,
                stderr=subprocess.STDOUT, start_new_session=True)
        record.update(status='running', coordinator_pid=child.pid)
        write(screen / 'supervision.json', record)
        while True:
            capture_tree(child.pid, identities)
            code = child.poll()
            if code is not None:
                record.update(status='complete' if code == 0 else 'failed', returncode=code)
                break
            if time.monotonic() - began >= seconds:
                raise TimeoutError(f'fresh-supervisor.hard-time-limit: {seconds}')
            time.sleep(1)
    except BaseException as error:
        record.update(status='incomplete', diagnostic=dict(code='fresh-supervisor.stopped',
            severity='error', stage='process-supervision', entity=str(screen), message=str(error)))
        raise
    finally:
        try:
            if child is not None:
                stop_tree(child, identities)
        except BaseException as error:
            record.update(status='incomplete', cleanup_error=str(error))
            raise
        finally:
            record['seconds'] = time.monotonic() - began
            write(screen / 'supervision.json', record)


if __name__ == '__main__':
    def interrupted(signum, _):
        raise InterruptedError(f'fresh-supervisor.interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    run(parser.parse_args())
