"""Bounded pipe collection with task-order results and no blocking reader threads."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import time

from ppo_harness_state import SCHEMA, require, read, sha, owned, HarnessError


def accept_result(root, task_path, envelope, reused=False):
    task_path = owned(root, task_path)
    require(envelope.get('kind') == 'result' and owned(root, envelope['task']) == task_path,
            'result-envelope', f'result belongs to another task: {task_path}', 'collection', task_path)
    path = owned(root, envelope['path'])
    require(path == task_path.with_name('result.json') and sha(path) == envelope['sha256'],
            'result-envelope', f'wrong result path/hash: {task_path}', 'collection', task_path)
    result, task = read(path), read(task_path)
    require(result['schema'] == SCHEMA and result['input_sha256'] == sha(task_path) and
            all(result[key] == task[key] for key in ('identity', 'candidate', 'protocol_sha256', 'policy_sha256', 'critic_sha256')),
            'result-identity', f'result/behavior identity mismatch: {task_path}', 'collection', task_path)
    require(result['status'] in ('complete', 'generation-rejected'), 'result-status', 'unsupported worker outcome', 'collection', path)
    if result['status'] == 'complete':
        require(result['row']['budget_complete'] and result['row']['selected'] == task['horizon'],
                'result-budget', 'incomplete episode cannot enter learning', 'collection', path)
        if task['training']:
            experience = owned(root, result['experience'])
            require(experience == task_path.with_name('experience.pt.gz'), 'experience-path', 'wrong experience location', 'collection', path)
            if reused:
                require(sha(experience) == result['experience_sha256'], 'experience-identity',
                        'retained experience changed before resume', 'collection', experience)
    return dict(result, result_path=str(path), result_sha256=envelope['sha256'])


class Pool:
    def __init__(self, root, directory, config, progress, worker_script=None):
        self.root, self.config, self.progress = Path(root), config, progress
        self.children, self.logs, self.buffers = [], [], {}
        self.selector = selectors.DefaultSelector()
        self.pending_messages = []
        directory.mkdir(parents=True)
        began = time.monotonic()
        try:
            environment = dict(os.environ, OMP_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1', MKL_NUM_THREADS='1',
                               NUMEXPR_NUM_THREADS='1', PYTHONHASHSEED='0')
            # Collectors never need cloud credentials. They remain in the
            # coordinator's owned process group for supervisor cleanup.
            for key in ('WANDB_API_KEY', 'WANDB_ENTITY', 'WANDB_PROJECT'):
                environment.pop(key, None)
            for slot in range(config['execution']['workers']):
                log = open(directory / f'worker-{slot}.log', 'wb')
                self.logs.append(log)
                child = subprocess.Popen([sys.executable, str(worker_script or Path(__file__).with_name('ppo_harness_worker.py')),
                                          '--run', str(root)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log,
                                         env=environment, bufsize=0)
                self.children.append(child)
                self.selector.register(child.stdout, selectors.EVENT_READ, slot)
                self.buffers[slot] = b''
            ready = set()
            while len(ready) < len(self.children):
                self.progress(stage='worker-startup', completed=len(ready), total=len(self.children), advanced=False)
                require(time.monotonic() - began < config['limits']['startup_seconds'], 'startup-timeout',
                        'workers did not become ready within budget', 'collection', root)
                for slot, message in self.receive():
                    require(message.get('kind') == 'ready' and slot not in ready, 'worker-startup', 'unexpected ready message', 'collection', slot)
                    ready.add(slot)
                    self.progress(stage='worker-startup', completed=len(ready), total=len(self.children), advanced=True)
            self.startup_seconds = time.monotonic() - began
        except BaseException:
            self.close()
            raise

    def receive(self):
        messages = []
        for key, _ in self.selector.select(timeout=.1):
            slot = key.data
            data = os.read(key.fileobj.fileno(), 65536)
            require(bool(data), 'worker-eof', f'worker {slot} exited without a result', 'collection', self.children[slot].pid)
            self.buffers[slot] += data
            require(len(self.buffers[slot]) <= 1024 * 1024, 'worker-message-size', 'worker pipe exceeded message bound', 'collection', slot)
            while b'\n' in self.buffers[slot]:
                line, self.buffers[slot] = self.buffers[slot].split(b'\n', 1)
                message = json.loads(line)
                if message.get('kind') == 'error':
                    error = HarnessError('worker-failed', f'worker {slot} failed', 'collection', slot)
                    error.diagnostic = message['diagnostic']
                    raise error
                messages.append((slot, message))
        return messages

    def collect(self, tasks):
        began = time.monotonic()
        pending, results, completion = {}, {}, []
        remaining = iter(enumerate(tasks))
        def assign(slot):
            item = next(remaining, None)
            if item is not None:
                index, path = item
                self.children[slot].stdin.write((json.dumps(dict(op='collect', task=str(path))) + '\n').encode())
                pending[slot] = (index, path, time.monotonic())
        for slot in range(min(len(tasks), len(self.children))):
            assign(slot)
        while pending:
            self.progress(stage='collection', completed=len(results), total=len(tasks), advanced=False)
            now = time.monotonic()
            require(now - began < self.config['limits']['batch_seconds'], 'batch-timeout', 'collection exceeded phase budget', 'collection')
            for slot, (_, path, assigned) in pending.items():
                require(now - assigned < self.config['limits']['task_seconds'], 'worker-timeout',
                        f'worker {slot} exceeded its task budget', 'collection', path)
            for slot, message in self.receive():
                require(slot in pending, 'unexpected-result', 'idle worker sent a result', 'collection', slot)
                index, path, _ = pending.pop(slot)
                results[index] = accept_result(self.root, path, message)
                completion.append(index)
                self.progress(stage='collection', completed=len(results), total=len(tasks), advanced=True)
                assign(slot)
        return [results[i] for i in range(len(tasks))], dict(seconds=time.monotonic() - began,
            completion_order=completion, startup_seconds=self.startup_seconds)

    def close(self):
        deadline = time.monotonic() + self.config['limits']['cleanup_seconds']
        for child in self.children:
            try:
                if child.poll() is None:
                    child.stdin.write(b'{"op":"stop"}\n')
                child.stdin.close()
            except (BrokenPipeError, OSError):
                pass
        for child in self.children:
            try:
                child.wait(timeout=max(.001, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=2)
            child.stdout.close()
        self.selector.close()
        for log in self.logs:
            log.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
