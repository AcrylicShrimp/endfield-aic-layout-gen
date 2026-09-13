"""Persistent blueprint workers; central ordered sampling and parallel native work."""
import os
from pathlib import Path
import selectors
import socket
import subprocess
import sys
import time

import torch

from blueprint_execution_worker import SCHEMA
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_update import digest, require
from ppo_harness_transport import Reader, receive, send


class BlueprintPool:
    def __init__(self, config, library, cores, output, guard, saved=None, message_bytes=512 * 1024**2, *, worker_entry=None, policy=None):
        require(sys.platform == 'linux', 'replay-balance.execution-gate: Linux shared-memory workers required')
        require(len(cores) == config['training']['streams'] and len(set(cores)) == len(cores),
                'replay-balance.execution-gate: one distinct CPU per stream')
        require(set(cores) <= os.sched_getaffinity(0), 'replay-balance.execution-gate: worker CPU outside allocation')
        if saved is not None:
            require([s['index'] for s in saved] == list(range(len(cores))), 'worker restore stream ordering')
        self.config, self.guard, self.policy = config, guard, policy
        worker_entry = worker_entry or Path(__file__).with_name('blueprint_execution_worker.py')
        self.workers, self.sockets, self.readers, self.logs = [], [], [], []
        self.selector = selectors.DefaultSelector()
        self.phase_seconds = {}
        self.request_id = 0
        self.worker_cpu_seconds = [0.] * len(cores)
        self.max_step_concurrency = 0
        self.output = Path(output)
        self.output.mkdir(parents=True, exist_ok=True)
        try:
            for index, core in enumerate(cores):
                parent, child = socket.socketpair()
                log = (self.output / f'worker-{index}.log').open('ab')
                self.logs.append(log)
                try:
                    process = subprocess.Popen([sys.executable, str(worker_entry),
                        '--fd', str(child.fileno()), '--core', str(core), '--message-bytes', str(message_bytes),
                        '--failure-directory', str(self.output / 'partial')], pass_fds=(child.fileno(),),
                        stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                        env=dict(os.environ, OMP_NUM_THREADS='1', MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1'))
                except BaseException:
                    parent.close()
                    raise
                finally:
                    child.close()
                self.workers.append(process)
                self.sockets.append(parent)
                self.readers.append(Reader(message_bytes, owned_buffers=policy is not None))
                self.selector.register(parent, selectors.EVENT_READ, index)
            self.identities = self.exchange([dict(operation='initialize', config=config, library=str(library), index=i,
                saved=None if saved is None else saved[i]) for i in range(len(cores))])
            require(len({r['pid'] for r in self.identities}) == len(cores)
                    and all(r['affinity'] == [core] for r, core in zip(self.identities, cores, strict=True)),
                    'replay-balance.execution-gate: worker identity/affinity mismatch')
        except BaseException:
            self.close()
            raise

    def exchange(self, commands):
        """One outstanding bounded command per worker, returned in stream order."""
        require(len(commands) <= len(self.workers), 'worker command coverage')
        pending = set(range(len(commands)))
        results = [None] * len(commands)
        intervals = []
        self.request_id += 1
        for index, command in enumerate(commands):
            self.guard()
            send(dict(command, schema=SCHEMA, request_id=self.request_id), self.sockets[index])
        while pending:
            self.guard()
            for key, _ in self.selector.select(.1):
                index = key.data
                require(index in pending, 'replay-balance.worker-lost: unexpected worker response')
                data, descriptors = receive(key.fileobj)
                if not data:
                    for fd in descriptors:
                        os.close(fd)
                    raise RuntimeError(f'replay-balance.worker-lost: worker {index} disconnected; see {self.output}')
                messages = self.readers[index].feed(data, descriptors)
                require(len(messages) <= 1, 'replay-balance.worker-lost: duplicate worker response')
                if messages:
                    message = messages[0]
                    require(message['ok'], f'replay-balance.worker-lost: worker {index}: {message.get("error")}')
                    require(message['schema'] == SCHEMA and message['request_id'] == self.request_id
                            and message['worker_index'] == index, 'replay-balance.worker-lost: response identity mismatch')
                    results[index] = message['result']
                    operation = commands[index]['operation']
                    self.phase_seconds[operation] = self.phase_seconds.get(operation, 0.) + message['seconds']
                    self.worker_cpu_seconds[index] += message['cpu_seconds']
                    if operation == 'step':
                        intervals.append(message['interval_ns'])
                    pending.remove(index)
            for index in pending:
                require(self.workers[index].poll() is None,
                        f'replay-balance.worker-lost: worker {index} exited; see {self.output}')
        active = 0
        for _, delta in sorted([(start, 1) for start, _ in intervals] + [(end, -1) for _, end in intervals]):
            active += delta
            self.max_step_concurrency = max(self.max_step_concurrency, active)
        return results

    def snapshot(self):
        return [r['stream'] for r in self.exchange([dict(operation='snapshot') for _ in self.workers])]

    def collect(self, model, sampler):
        require(self.policy is not None or next(model.parameters()).device.type == 'cpu',
                'default ordered policy sampler requires a CPU behavior copy')
        began, behavior = time.perf_counter(), digest(model.state_dict())
        replies = self.exchange([dict(operation='begin') for _ in self.workers])
        inputs=[]
        for _ in range(self.config['training']['chunk_actions']):
            if self.policy is not None:
                inputs.append([r['input'] for r in replies])
            if self.policy is not None:
                with torch.no_grad():
                    commands = self.policy.sample(model, [r['input'] for r in replies], sampler)
                commands = [dict(operation='step', **row) for row in commands]
            else:
                batch = pack([r['input'] for r in replies])
                with torch.no_grad():
                    logits, values = model.forward_packed(batch)
                    logp, entropy = distribution(logits, batch)
                offset, commands = 0, []
                for count, value, ent in zip(batch['action_counts'], values, entropy, strict=True):
                    index = int(torch.multinomial(logp[offset:offset + count].exp(), 1, generator=sampler))
                    commands.append(dict(operation='step', index=index, old_logp=float(logp[offset + index]),
                                         old_value=float(value), entropy=float(ent)))
                    offset += count
            replies = self.exchange(commands)
        with torch.no_grad():
            if self.policy is None:
                _, values = model.forward_packed(pack([r['input'] for r in replies]))
            else:
                values = self.policy.values(model, [r['input'] for r in replies])
        chunks = self.exchange([dict(operation='finish', value=float(v)) for v in values])
        require(digest(model.state_dict()) == behavior, 'collection changed frozen behavior parameters')
        return dict(schema=self.config.get('chunk_schema', 'blueprint-ppo-chunk-v1'), model_before=behavior,
            native_before=[c['native_before'] for c in chunks],
            samples=[dict(c['samples'][step],input=inputs[step][i]) if self.policy is not None else c['samples'][step]
                for step in range(self.config['training']['chunk_actions']) for i,c in enumerate(chunks)],
            frontiers=[c['frontier'] for c in chunks], completed=[c['completed'] for c in chunks if c['completed'] is not None],
            collection_seconds=time.perf_counter() - began)

    def assign_starts(self, slots):
        require(len(slots)==len(self.workers), 'declarative.curriculum-plan: full worker slot coverage required')
        return self.exchange([dict(operation='start',start=slot) for slot in slots])

    def evaluate(self, model, cases, seed, update, output, deadline, evaluation=None, *, uniform=False):
        self.exchange([dict(operation='model', state=model.state_dict()) for _ in self.workers])
        rows = []
        for start in range(0, len(cases), len(self.workers)):
            commands = [dict(operation='evaluate', case=case, seed=seed, update=update, output=str(output),
                deadline=deadline, evaluation=evaluation or self.config['evaluation'], uniform=uniform,
                action_seed=case['action_seed'], sample=case.get('sample', 0)) for case in cases[start:start + len(self.workers)]]
            rows.extend(r['row'] for r in self.exchange(commands))
        return rows

    def close(self):
        for process, stream in zip(self.workers, self.sockets):
            if process.poll() is None:
                try:
                    send(dict(operation='close'), stream)
                except OSError:
                    pass
        deadline = time.monotonic() + 3
        for process in self.workers:
            try:
                process.wait(timeout=max(.01, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                process.terminate()
        for process in self.workers:
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        for stream in self.sockets:
            stream.close()
        for reader in self.readers:
            reader.close()
        for log in self.logs:
            log.close()
        self.selector.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
