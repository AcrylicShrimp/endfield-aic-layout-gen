"""Bounded process collection of one immutable four-episode on-policy group."""
import argparse
from copy import deepcopy
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path = Path(path)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')
    temporary.replace(path)


class CollectionError(RuntimeError):
    def __init__(self, code, message, *, group=None, attempt=None):
        super().__init__(message)
        self.diagnostic = dict(code=code, severity='error', stage='on-policy-collection',
                               group=group, attempt=attempt, message=message)


def processes():
    result = subprocess.run(['ps', '-axo', 'pid=,ppid=,pgid=,rss='],
                            capture_output=True, text=True, check=True, timeout=2)
    return [tuple(map(int, line.split())) for line in result.stdout.splitlines() if line.strip()]


class ProcessTreeMemory:
    """Sample summed RSS; shared pages may be counted by more than one process."""
    def __init__(self, interval=.2):
        self.root = os.getpid()
        self.interval = interval
        self.groups = set()
        self.peak_bytes = self.peak_processes = self.samples = 0
        self.errors = []
        self.stop_event = threading.Event()
        self.thread = None

    def add_group(self, pgid):
        # Replace the set so the sampler can read a consistent snapshot.
        self.groups = self.groups | {pgid}

    def sample(self):
        try:
            rows = processes()
            selected = {self.root} | {pid for pid, _, pgid, _ in rows if pgid in self.groups}
            while True:
                children = {pid for pid, parent, _, _ in rows if parent in selected}
                if children <= selected:
                    break
                selected |= children
            measured = [(pid, rss) for pid, _, _, rss in rows if pid in selected]
            self.peak_bytes = max(self.peak_bytes, sum(rss for _, rss in measured) * 1024)
            self.peak_processes = max(self.peak_processes, len(measured))
            self.samples += 1
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            self.errors.append(str(error))

    def __enter__(self):
        self.sample()
        def loop():
            while not self.stop_event.wait(self.interval):
                self.sample()
        self.thread = threading.Thread(target=loop, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop_event.set()
        self.thread.join(timeout=3)
        if self.thread.is_alive():
            self.errors.append('RSS sampler did not finish within its ps timeout')
        self.sample()

    def report(self):
        return dict(method='ps sampled sum of coordinator and descendant/owned-process-group RSS',
                    interval_seconds=self.interval, peak_bytes=self.peak_bytes,
                    peak_processes=self.peak_processes, samples=self.samples, errors=self.errors,
                    scope='Sampled RSS including the ps measurement helper, not an allocation bound or unique physical memory; shared pages can be counted repeatedly.')


def stop_owned(children):
    """Terminate every owned session, including runtime children of failed workers."""
    def send(child, signum):
        members = [pid for pid, _, pgid, _ in processes() if pgid == child.pid]
        if not members:
            return
        try:
            os.killpg(child.pid, signum)
        except ProcessLookupError:
            pass
        except PermissionError:
            # Recheck membership before falling back to individual signalling.
            # Never signal a PID that has left the owned process group.
            for pid in members:
                try:
                    if os.getpgid(pid) == child.pid:
                        os.kill(pid, signum)
                except (ProcessLookupError, PermissionError):
                    pass
    for child in children:
        send(child, signal.SIGTERM)
    deadline = time.monotonic() + 5
    for child in children:
        try:
            child.wait(timeout=max(.001, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            pass
    for child in children:
        send(child, signal.SIGKILL)
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
    groups = {child.pid for child in children}
    # macOS reaps orphaned descendants asynchronously; allow a bounded interval.
    deadline = time.monotonic() + 2
    while True:
        remaining = [pid for pid, _, pgid, _ in processes() if pgid in groups]
        if not remaining or time.monotonic() >= deadline:
            return remaining
        time.sleep(.02)


def _validate_result(job):
    result = read(job['result'])
    if (result.get('identity') != job['identity'] or result.get('input_sha256') != sha(job['path'])
            or result.get('policy_sha256') != job['identity']['policy_sha256']):
        raise CollectionError('on-policy.worker-identity', f'Worker result identity mismatch: {job["result"]}',
                              group=job['identity']['group'], attempt=job['identity']['attempt'])
    if result.get('status') != 'complete':
        raise CollectionError('on-policy.worker-failed', f'Incomplete worker result: {job["result"]}',
                              group=job['identity']['group'], attempt=job['identity']['attempt'])
    row = result['row']
    if (row.get('case') != job['identity']['case'] or row.get('bank') != job['identity']['seed']
            or row.get('directory') != job['input']['directory']
            or row.get('selected') != job['horizon'] or not row.get('budget_complete')):
        raise CollectionError('on-policy.group-incomplete', f'Incomplete or mismatched episode: {job["result"]}',
                              group=job['identity']['group'], attempt=job['identity']['attempt'])
    for name, expected in result['artifacts_sha256'].items():
        if sha(Path(row['directory']) / name) != expected:
            raise CollectionError('on-policy.worker-identity', f'Changed episode artifact: {name}',
                                  group=job['identity']['group'], attempt=job['identity']['attempt'])
    if set(result['artifacts_sha256']) != {'episode.json.gz', 'states.json.gz', 'best.json.gz'}:
        raise CollectionError('on-policy.worker-identity', f'Missing episode artifacts: {job["result"]}',
                              group=job['identity']['group'], attempt=job['identity']['attempt'])
    return result


def _run_jobs(jobs, workers, deadline, *, command, memory, rss_limit_bytes, guard=lambda: None):
    """One-shot workers; no retries, shared runtime or partially returned group."""
    pending = list(jobs)
    active, owned, completed, completion_order = {}, [], {}, []
    logs = []
    error = None
    try:
        while pending or active:
            guard()
            if memory.errors:
                raise CollectionError('on-policy.collection-memory-measurement', str(memory.errors))
            if time.monotonic() >= deadline:
                raise CollectionError('on-policy.parallel-resource-bound', 'Collection group/study deadline expired')
            if memory.peak_bytes > rss_limit_bytes:
                raise CollectionError('on-policy.parallel-resource-bound',
                                      f'Sampled process RSS exceeds {rss_limit_bytes} bytes: {memory.peak_bytes}')
            while pending and len(active) < workers:
                guard()
                if time.monotonic() >= deadline or memory.peak_bytes > rss_limit_bytes:
                    raise CollectionError('on-policy.parallel-resource-bound', 'Collection bound expired before dispatch')
                job = pending.pop(0)
                assigned = time.monotonic()
                job['input']['assigned_monotonic'] = assigned
                write(job['path'], job['input'])
                log = open(job['log'], 'w')
                logs.append(log)
                environment = dict(os.environ, OMP_NUM_THREADS='1', MKL_NUM_THREADS='1',
                                   OPENBLAS_NUM_THREADS='1', VECLIB_MAXIMUM_THREADS='1',
                                   NUMEXPR_NUM_THREADS='1', PYTHONHASHSEED='0')
                child = subprocess.Popen([*command, '--job', str(job['path'])], stdout=log,
                                         stderr=subprocess.STDOUT, env=environment, start_new_session=True)
                owned.append(child)
                job['pid'] = child.pid
                memory.add_group(child.pid)
                active[child.pid] = (child, job, assigned)
            for pid, (child, job, assigned) in list(active.items()):
                exit_code = child.poll()
                if exit_code is None:
                    if time.monotonic() - assigned >= job['input']['episode_seconds']:
                        raise CollectionError('on-policy.parallel-resource-bound',
                                              f'Worker episode deadline expired: {job["log"]}',
                                              group=job['identity']['group'], attempt=job['identity']['attempt'])
                    continue
                if exit_code:
                    raise CollectionError('on-policy.worker-failed',
                                          f'Worker exited {exit_code}: {job["log"]}',
                                          group=job['identity']['group'], attempt=job['identity']['attempt'])
                observed_seconds = time.monotonic() - assigned
                if observed_seconds >= job['input']['episode_seconds']:
                    raise CollectionError('on-policy.parallel-resource-bound',
                                          f'Worker completion exceeded its episode deadline: {job["log"]}',
                                          group=job['identity']['group'], attempt=job['identity']['attempt'])
                try:
                    result = _validate_result(job)
                except (OSError, ValueError, KeyError, TypeError, AttributeError) as caught:
                    raise CollectionError('on-policy.worker-identity', f'Invalid worker result {job["result"]}: {caught}',
                                          group=job['identity']['group'], attempt=job['identity']['attempt']) from caught
                result['dispatch_to_completion_seconds'] = observed_seconds
                completed[job['identity']['attempt']] = result
                completion_order.append(job['identity']['attempt'])
                del active[pid]
            if pending or active:
                time.sleep(.02)
        return [completed[job['identity']['attempt']] for job in jobs], completion_order
    except BaseException as caught:
        error = caught
        raise
    finally:
        try:
            remaining = stop_owned(owned)
        finally:
            for log in logs:
                log.close()
        if remaining:
            cleanup = CollectionError('on-policy.worker-cleanup', f'Owned processes survived cleanup: {remaining}')
            if error is not None:
                cleanup.add_note(f'Original collection failure: {error}')
            raise cleanup


def collect_group(protocol_path, checkpoint_path, case_id, attempts, output, *, group,
                  workers, group_seconds, policy_sha256, rss_limit_bytes,
                  episode_seconds=None, guard=lambda: None, memory=None):
    """Return all four ordered episodes or raise, preserving failed-group evidence."""
    began = time.monotonic()
    protocol_path, checkpoint_path, output = map(Path, (protocol_path, checkpoint_path, output))
    if os.name != 'posix':
        raise CollectionError('on-policy.collection-platform', 'Owned worker sessions require POSIX', group=group)
    protocol = read(protocol_path)
    if (workers not in (1, 2, 4) or type(workers) is not int or protocol['config']['group_episodes'] != 4
            or len(attempts) != 4 or sorted(a['attempt'] for a in attempts) != list(range(4))
            or any(type(a['attempt']) is not int or type(a['seed']) is not int or not 0 <= a['seed'] < 2**63 for a in attempts)
            or len({a['seed'] for a in attempts}) != 4):
        raise CollectionError('on-policy.worker-identity', 'Expected four distinct attempts/seeds and 1, 2 or 4 workers', group=group)
    if case_id not in {case['id'] for case in protocol['cases']}:
        raise CollectionError('on-policy.worker-identity', f'Unknown reset case: {case_id}', group=group)
    episode_seconds = protocol['config']['episode_seconds'] if episode_seconds is None else episode_seconds
    if any(type(value) not in (int, float) or not math.isfinite(value) or value <= 0
           for value in (group_seconds, episode_seconds)):
        raise CollectionError('on-policy.parallel-resource-bound', 'Expected finite positive time bounds', group=group)
    if (type(rss_limit_bytes) is not int or rss_limit_bytes <= 0
            or not isinstance(policy_sha256, str) or len(policy_sha256) != 64
            or any(c not in '0123456789abcdef' for c in policy_sha256)):
        raise CollectionError('on-policy.worker-identity', 'Invalid policy hash or sampled-RSS bound', group=group)
    output.mkdir()
    hashes = dict(protocol_sha256=sha(protocol_path), checkpoint_sha256=sha(checkpoint_path),
                  worker_source_sha256=sha(__file__))
    jobs = []
    for attempt in sorted(attempts, key=lambda a: a['attempt']):
        identity = dict(group=group, attempt=attempt['attempt'], seed=attempt['seed'], case=case_id,
                        policy_sha256=policy_sha256, **hashes)
        stem = output / f'worker-{attempt["attempt"]}'
        result_path = stem.with_suffix('.json')
        data = dict(identity=identity, protocol=str(protocol_path.resolve()), checkpoint=str(checkpoint_path.resolve()),
                    directory=str((output / f'episode-{attempt["attempt"]}').resolve()),
                    result=str(result_path.resolve()), episode_seconds=episode_seconds)
        jobs.append(dict(identity=identity, input=data, path=stem.with_suffix('.input.json'),
                         result=result_path, log=stem.with_suffix('.log'), horizon=protocol['config']['horizon']))
    owned_memory = memory is None
    memory = ProcessTreeMemory() if memory is None else memory
    if owned_memory:
        memory.__enter__()
    try:
        results, order = _run_jobs(jobs, workers, began + group_seconds,
                                   command=[sys.executable, str(Path(__file__).resolve()), 'worker'],
                                   guard=guard, memory=memory, rss_limit_bytes=rss_limit_bytes)
        if owned_memory:
            memory.__exit__()
            write(output / 'memory.json', memory.report())
            owned_memory = False
        guard()
        if memory.errors:
            raise CollectionError('on-policy.collection-memory-measurement', str(memory.errors), group=group)
        if time.monotonic() - began >= group_seconds or memory.peak_bytes > rss_limit_bytes:
            raise CollectionError('on-policy.parallel-resource-bound', 'Group bound exhausted during validation/cleanup', group=group)
        if hashes != dict(protocol_sha256=sha(protocol_path), checkpoint_sha256=sha(checkpoint_path),
                          worker_source_sha256=sha(__file__)):
            raise CollectionError('on-policy.worker-identity', 'Collection inputs changed during execution', group=group)
        record = dict(schema_version=1, status='complete', group=group, case=case_id, workers=workers,
                      requested_episodes=4, completed_episodes=len(results), selected=sum(r['row']['selected'] for r in results),
                      completion_order=order, results=results, collection_seconds=time.monotonic() - began,
                      optimizer_updates=0, **hashes)
        write(output / 'collection.json', record)
        return [r['row'] for r in results], record
    except BaseException as error:
        diagnostic = error.diagnostic if isinstance(error, CollectionError) else dict(
            code='on-policy.worker-failed', severity='error', stage='on-policy-collection', group=group, message=str(error))
        diagnostic = dict(diagnostic, group=group if diagnostic.get('group') is None else diagnostic['group'])
        write(output / 'partial.json', dict(schema_version=1, status='incomplete', diagnostic=diagnostic,
                                           optimizer_updates=0, seconds=time.monotonic() - began,
                                           owned_process_groups=[j['pid'] for j in jobs if 'pid' in j],
                                           retained_worker_results=[str(j['result']) for j in jobs if j['result'].exists()]))
        raise
    finally:
        if owned_memory:
            memory.__exit__()
            write(output / 'memory.json', memory.report())


def worker(job_path):
    """Load a fresh model/runtime; startup is charged to the assigned episode."""
    data = read(job_path)
    identity = data['identity']
    def interrupted(signum, _):
        raise CollectionError('on-policy.worker-cancelled', f'Worker received signal {signum}',
                              group=identity['group'], attempt=identity['attempt'])
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        if (sha(data['protocol']) != identity['protocol_sha256']
                or sha(data['checkpoint']) != identity['checkpoint_sha256']
                or sha(__file__) != identity['worker_source_sha256']):
            raise ValueError('on-policy.worker-source-identity')
        import torch
        from full_factory_large_learning import verify_sources
        from full_factory_on_policy import on_policy_rollout, parameter_hash
        from full_factory_policy import FullFactoryPolicy, SCHEMA
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        torch.use_deterministic_algorithms(True)
        protocol = read(data['protocol'])
        verify_sources(protocol)
        checkpoint = torch.load(data['checkpoint'], map_location='cpu', weights_only=False)
        if checkpoint['schema'] != SCHEMA or (checkpoint['depth'], checkpoint['width']) != (protocol['depth'], protocol['width']):
            raise ValueError('on-policy.worker-checkpoint-schema')
        model = FullFactoryPolicy(protocol['depth'], protocol['width'])
        model.load_state_dict(checkpoint['policy'], strict=True)
        torch.set_rng_state(checkpoint['rng'])
        behavior = parameter_hash(model)
        if behavior != checkpoint['policy_sha256'] or behavior != identity['policy_sha256']:
            raise ValueError('on-policy.worker-policy-identity')
        startup = time.monotonic() - data['assigned_monotonic']
        remaining = data['episode_seconds'] - startup
        if remaining <= 0:
            raise TimeoutError('on-policy.worker-startup-budget')
        # The existing rollout charges all environment work. Subtract process and
        # policy startup here without changing its action or gradient semantics.
        local = deepcopy(protocol)
        local['config']['episode_seconds'] = remaining
        case = next(c for c in protocol['cases'] if c['id'] == identity['case'])
        row, summary = on_policy_rollout(local, case, model, identity['seed'], Path(data['directory']))
        elapsed = time.monotonic() - data['assigned_monotonic']
        row['budget_complete'] = row['budget_complete'] and elapsed < data['episode_seconds']
        if summary['policy_sha256'] != behavior or parameter_hash(model) != behavior:
            raise ValueError('on-policy.worker-policy-mutated')
        verify_sources(protocol)
        artifacts = {name: sha(Path(data['directory']) / name) for name in
                     ('episode.json.gz', 'states.json.gz', 'best.json.gz')}
        write(data['result'], dict(schema_version=1, status='complete', identity=identity,
                                  input_sha256=sha(job_path), row=row, policy_sha256=behavior, pid=os.getpid(),
                                  startup_seconds=startup, worker_seconds=elapsed,
                                  effective_rollout_seconds=remaining, artifacts_sha256=artifacts))
    except BaseException as error:
        write(Path(data['result']).with_suffix('.partial.json'), dict(status='incomplete', identity=identity,
              code='on-policy.worker-failed', severity='error', stage='on-policy-collection', message=str(error)))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('worker').add_argument('--job', required=True, type=Path)
    args = parser.parse_args()
    worker(args.job)
