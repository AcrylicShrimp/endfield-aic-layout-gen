"""Single-owner process supervision, resource bounds and explicit stop/resume."""
import json
import math
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
import uuid

from ppo_harness_state import (SCHEMA, HarnessError, require, read, write, sha, diagnostic, writer_lock,
                               check_sources, load_state, checked_path, mkdir)

SESSION_KEY = 'AIC_HARNESS_SESSION'


def identity(pid):
    path = Path(f'/proc/{pid}/stat')
    try:
        if path.is_file():
            fields = path.read_text().rsplit(')', 1)[1].split()
            return dict(pid=pid, start=fields[19])
        value = subprocess.run(['ps', '-p', str(pid), '-o', 'lstart='], capture_output=True,
                               text=True, timeout=2, check=False).stdout.strip()
        return dict(pid=pid, start=value) if value else None
    except (FileNotFoundError, ProcessLookupError):
        return None


def process_rows():
    result = subprocess.run(['ps', '-axo', 'pid=,ppid=,pgid=,rss=,pcpu=,stat='], capture_output=True,
                            text=True, timeout=2, check=True)
    return [(int(a), int(b), int(c), int(d), float(e)) for a, b, c, d, e, status in
            (line.split() for line in result.stdout.splitlines() if line.strip()) if not status.startswith('Z')]


def members(group):
    return [row for row in process_rows() if row[2] == group]


def has_session(pid, session):
    token = f'{SESSION_KEY}={session}'.encode()
    try:
        if Path('/proc').is_dir():
            return token in Path(f'/proc/{pid}/environ').read_bytes().split(b'\0')
        # ps output stays in memory; never publish command/environment bytes.
        result = subprocess.run(['ps', 'eww', '-p', str(pid), '-o', 'command='], capture_output=True, timeout=2, check=False)
        return token in result.stdout.split()
    except (OSError, subprocess.SubprocessError):
        return False


def discover_owned(processes, session, rows=None, full_scan=False):
    """Include SDK services that create a new session, and lost launch records."""
    rows = process_rows() if rows is None else rows
    groups = {p['pid'] for p in processes}
    related = {r[0] for r in rows if r[2] in groups}
    while True:
        expanded = related | {r[0] for r in rows if r[1] in related}
        if expanded == related:
            break
        related = expanded
    candidates = rows if Path('/proc').is_dir() or full_scan else [r for r in rows if r[0] in related]
    found = list(processes)
    for pid, _, group, _, _ in candidates:
        if group != os.getpgrp() and group not in groups and has_session(pid, session):
            found.append(dict(name='owned-service', pid=group, identity=identity(group)))
            groups.add(group)
    return found


def group_is_owned(process, session):
    expected = process['identity']
    if expected is not None and identity(expected['pid']) == expected:
        return True
    for pid, _, _, _, _ in members(process['pid']):
        if has_session(pid, session):
            return True
    return False


def signal_group(process, session, signum):
    if members(process['pid']) and group_is_owned(process, session):
        try:
            os.killpg(process['pid'], signum)
        except ProcessLookupError:
            pass
        except PermissionError:
            # Darwin returns EPERM for a group containing only zombies. A
            # real permission failure with live members must remain visible.
            if members(process['pid']):
                raise


def cleanup(processes, session, seconds):
    processes[:] = discover_owned(processes, session, full_scan=True)
    for process in processes:
        signal_group(process, session, signal.SIGTERM)
    deadline = time.monotonic() + seconds
    while any(members(p['pid']) for p in processes) and time.monotonic() < deadline:
        # Popen handles are reaped by the caller; a zombie leader is harmless
        # here and the final post-wait check distinguishes live descendants.
        time.sleep(.1)
        discovered = discover_owned(processes, session)
        for process in discovered:
            if process not in processes:
                signal_group(process, session, signal.SIGTERM)
        processes[:] = discovered
    for process in processes:
        signal_group(process, session, signal.SIGKILL)
    deadline = time.monotonic() + 2
    while any(members(p['pid']) for p in processes) and time.monotonic() < deadline:
        time.sleep(.05)


def gpu_sample():
    try:
        result = subprocess.run(['nvidia-smi', '--query-gpu=index,utilization.gpu,memory.used,memory.total',
                                 '--format=csv,noheader,nounits'], capture_output=True, text=True, timeout=2, check=True)
        return [dict(index=int(a), utilization_percent=float(b), used_mib=float(c), total_mib=float(d))
                for a, b, c, d in (line.split(',') for line in result.stdout.splitlines())]
    except (OSError, ValueError, subprocess.SubprocessError):
        return None


def append_sample(directory, value, limit=2 * 1024 * 1024):
    path = directory / 'resources.jsonl'
    if path.exists() and path.stat().st_size >= limit:
        (directory / 'resources-2.jsonl').unlink(missing_ok=True)
        previous = directory / 'resources-1.jsonl'
        if previous.exists():
            previous.replace(directory / 'resources-2.jsonl')
        path.replace(previous)
    with open(path, 'a') as stream:
        stream.write(json.dumps(value, allow_nan=False) + '\n')


def validate_resume(root, protocol, state):
    require(state['cursor'] <= len(protocol['jobs']) and len(state['jobs']) <= len(protocol['jobs']),
            'resume-schedule', 'invalid job count/cursor', 'resume', root)
    for index, job in enumerate(state['jobs']):
        scheduled = protocol['jobs'][index]
        require(all(job[k] == scheduled[k] for k in ('id', 'candidate', 'seed')) and
                job['next_phase'] == len(job['records']) <= len(protocol['phases']),
                'resume-schedule', 'job or phase position differs', 'resume', job['id'])
        checked_path(root, job['checkpoint'])
        for ordinal, record in enumerate(job['records']):
            require(record['phase'] == protocol['phases'][ordinal], 'resume-record-order', 'phase order differs', 'resume', job['id'])
            checked_path(root, record)


def supervise(root, resume=False, duration_seconds=None):
    require(duration_seconds is None or (math.isfinite(duration_seconds) and duration_seconds > 0),
            'session-duration', 'duration seconds must be finite and positive', 'supervisor', root)
    root = Path(root).resolve()
    with writer_lock(root):
        old_path = root / 'supervisor.json'
        previous = read(old_path) if old_path.exists() else None
        if previous and previous['status'] == 'running':
            # Terminate the prior writer before reading its commit pointer or
            # rejecting drift. It may still commit while shutdown is pending.
            cleanup(previous['processes'], previous['session'], previous.get('cleanup_seconds', 5))
            require(not any(members(p['pid']) for p in previous['processes']), 'orphan-cleanup',
                    'owned processes remain from the lost session', 'resume', root)
        protocol, state = read(root / 'protocol.json'), load_state(root)
        require(protocol['schema'] == SCHEMA, 'protocol-schema', 'unsupported harness protocol', entity=root)
        check_sources(protocol)
        from ppo_harness_protocol import environment
        require(environment() == protocol['environment'], 'runtime-drift', 'Python/Node/Torch/runtime versions changed', 'supervisor', root)
        validate_resume(root, protocol, state)
        require(state['status'] != 'complete', 'already-complete', 'run has already completed', entity=root)
        if resume:
            require(state['sessions'] > 0, 'resume-unstarted', 'use run for a prepared experiment', entity=root)
        else:
            require(state['status'] == 'prepared' and state['sessions'] == 0, 'run-started', 'use resume for an existing execution', entity=root)
        limits = protocol['config']['limits']
        prior_seconds = state['elapsed_seconds']
        if previous and previous['status'] == 'running':
            # An abruptly lost supervisor has no trusted exit timestamp.
            # Charge its entire unclosed interval conservatively on resume.
            prior_seconds = max(prior_seconds, previous['prior_seconds'] + time.time() - previous['started_epoch'])
        require(prior_seconds < limits['run_seconds'], 'run-budget', 'cumulative run budget is exhausted', 'supervisor', root)
        require(shutil.disk_usage(root).free >= limits['disk_reserve_bytes'], 'disk-reserve', 'insufficient free disk before launch', 'supervisor', root)
        session = uuid.uuid4().hex
        directory = root / 'sessions' / session
        mkdir(directory)
        state.update(status='running', sessions=state['sessions'] + 1, elapsed_seconds=prior_seconds)
        write(root / 'state.json', state)
        began, started = time.monotonic(), time.time()
        status = dict(schema=SCHEMA, session=session, status='running', supervisor=identity(os.getpid()),
                      protocol_sha256=sha(root / 'protocol.json'), started_epoch=started, prior_seconds=prior_seconds,
                      updated_epoch=started, processes=[], elapsed_seconds=prior_seconds, diagnostic=None,
                      cleanup_seconds=limits['cleanup_seconds'])
        status['duration_seconds'] = duration_seconds
        write(old_path, status)
        stop = []
        def interrupted(signum, _):
            stop.append(f'signal {signum}')
        prior_handlers = {sig: signal.signal(sig, interrupted) for sig in (signal.SIGTERM, signal.SIGINT)}
        children, logs = [], []
        failure, normal_stop = None, None
        def launch(name, arguments, credentials=False):
            log = open(directory / f'{name}.log', 'wb')
            logs.append(log)
            environment = dict(os.environ, **{SESSION_KEY: session}, PYTHONUNBUFFERED='1')
            if not credentials:
                environment.pop('WANDB_API_KEY', None)
            child = subprocess.Popen([sys.executable, *arguments], stdout=log, stderr=subprocess.STDOUT,
                                     start_new_session=True, env=environment)
            children.append(child)
            status['processes'].append(dict(name=name, pid=child.pid, identity=identity(child.pid)))
            write(old_path, status)
            return child
        try:
            coordinator = launch('coordinator', [str(Path(__file__).with_name('ppo_harness_run.py')),
                                                 '--run', str(root), '--session', session])
            observer = launch('observer', [str(Path(__file__).with_name('ppo_harness_monitor.py')), 'watch',
                                          '--run', str(root), '--session', session], credentials=True)
            last_gpu, gpu = 0., None
            while coordinator.poll() is None:
                elapsed = prior_seconds + time.monotonic() - began
                request = root / 'stop-request.json'
                if stop or (request.exists() and read(request).get('session') == session):
                    normal_stop = 'user-stop'
                    raise InterruptedError(stop[0] if stop else 'CLI stop request')
                if elapsed >= limits['run_seconds'] or (duration_seconds is not None and time.monotonic() - began >= duration_seconds):
                    normal_stop = 'duration-reached'
                    raise InterruptedError('Configured training duration completed; preserving the last committed batch')
                rows = process_rows()
                status['processes'] = discover_owned(status['processes'], session, rows)
                groups = {p['pid'] for p in status['processes']}
                selected = [r for r in rows if r[0] == os.getpid() or r[2] in groups]
                rss, cpu = sum(r[3] for r in selected) * 1024, sum(r[4] for r in selected)
                free = shutil.disk_usage(root).free
                require(rss <= limits['rss_bytes'], 'rss-limit', f'aggregate RSS exceeded {limits["rss_bytes"]} bytes', 'supervisor', root)
                require(free >= limits['disk_reserve_bytes'], 'disk-reserve', 'free disk fell below reserve', 'supervisor', root)
                progress_path = root / 'progress.json'
                progress = read(progress_path) if progress_path.exists() else {}
                active = progress if progress.get('session') == session else {}
                require(time.time() - active.get('last_progress_epoch', started) < limits['progress_seconds'],
                        'progress-timeout', 'coordinator made no bounded progress', 'supervisor', root)
                if time.monotonic() - last_gpu >= 5:
                    gpu, last_gpu = gpu_sample(), time.monotonic()
                sample = dict(epoch=time.time(), elapsed_seconds=elapsed, rss_bytes=rss,
                              cpu_percent_lifetime=cpu, disk_free_bytes=free, gpu=gpu, processes=len(selected))
                status.update(updated_epoch=time.time(), elapsed_seconds=elapsed, resources=sample,
                              observer_exit_code=observer.poll())
                write(old_path, status)
                append_sample(directory, sample)
                time.sleep(.5)
            require(coordinator.returncode == 0 and load_state(root)['status'] == 'complete',
                    'coordinator-exit', f'coordinator exited with code {coordinator.returncode}', 'supervisor', root)
            # Let the observer publish the final committed points and URLs.
            try:
                observer.wait(timeout=limits['cleanup_seconds'])
            except subprocess.TimeoutExpired:
                status['observer_finalization'] = 'pending; committed records remain available for export'
        except BaseException as error:
            failure = diagnostic(error, 'supervisor', root)
            if normal_stop:
                failure = dict(code='ppo-harness.' + normal_stop, severity='info', stage='supervisor', entity=str(root), message=str(error))
            elif failure['code'] == 'ppo-harness.coordinator-exit':
                path = root / 'progress.json'
                prior = read(path) if path.exists() else {}
                if prior.get('session') == session and prior.get('diagnostic'):
                    failure = prior['diagnostic']
        finally:
            # The coordinator is the only state writer. Stop it first, publish
            # the terminal state, then let the observer flush and finish W&B.
            if normal_stop:
                write(root / 'stop-request.json', dict(session=session, reason=normal_stop, requested_epoch=time.time()))
            if children and children[0].poll() is None:
                signal_group(status['processes'][0], session, signal.SIGTERM)
                try:
                    children[0].wait(timeout=limits['cleanup_seconds'])
                except subprocess.TimeoutExpired:
                    signal_group(status['processes'][0], session, signal.SIGKILL)
                    children[0].wait(timeout=3)
            state = load_state(root)
            genuine_failure = failure is not None and failure.get('severity') == 'error'
            state.update(status='failed' if genuine_failure else 'stopped' if normal_stop else 'complete',
                         termination_reason='error' if genuine_failure else normal_stop or 'schedule-complete',
                         elapsed_seconds=prior_seconds + time.monotonic() - began, diagnostic=failure)
            write(root / 'state.json', state)
            if len(children) > 1 and children[1].poll() is None:
                try:
                    children[1].wait(timeout=limits['cleanup_seconds'])
                except subprocess.TimeoutExpired:
                    status['observer_finalization'] = 'pending; committed records remain available for export'
            cleanup(status['processes'], session, limits['cleanup_seconds'])
            for child in children:
                try:
                    child.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    failure = dict(code='ppo-harness.cleanup-failed', severity='error', stage='supervisor',
                                   entity=str(child.pid), message='owned child survived bounded cleanup')
            live = [row[0] for p in status['processes'] for row in members(p['pid'])]
            if live:
                failure = dict(code='ppo-harness.cleanup-failed', severity='error', stage='supervisor',
                               entity=str(root), message=f'owned descendants remain: {live}')
            for log in logs:
                log.close()
            state = load_state(root)
            genuine_failure = failure is not None and failure.get('severity') == 'error'
            state.update(status='failed' if genuine_failure else 'stopped' if normal_stop else 'complete',
                         termination_reason='error' if genuine_failure else normal_stop or 'schedule-complete',
                         elapsed_seconds=prior_seconds + time.monotonic() - began, diagnostic=failure)
            progress_path = root / 'progress.json'
            progress = read(progress_path) if progress_path.exists() else {}
            if progress.get('session') == session:
                for job in state['jobs']:
                    if job['id'] == progress.get('job'):
                        job['elapsed_seconds'] = max(job['elapsed_seconds'], progress['job_elapsed_seconds'])
            write(root / 'state.json', state)
            status.update(status=state['status'], updated_epoch=time.time(), elapsed_seconds=state['elapsed_seconds'],
                          termination_reason=state['termination_reason'],
                          diagnostic=failure, child_exit_codes=[child.returncode for child in children],
                          observer_exit_code=children[1].returncode if len(children) > 1 else None)
            write(old_path, status)
            for sig, handler in prior_handlers.items():
                signal.signal(sig, handler)
        return status


def stop_run(root):
    root = Path(root).resolve()
    status = read(root / 'supervisor.json')
    require(status['status'] == 'running' and identity(status['supervisor']['pid']) == status['supervisor'],
            'stop-inactive', 'no matching live supervisor', 'stop', root)
    write(root / 'stop-request.json', dict(session=status['session'], reason='user-stop', requested_epoch=time.time()))
    return dict(status='stop-requested', session=status['session'])
