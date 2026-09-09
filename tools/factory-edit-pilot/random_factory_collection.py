"""Persistent, bounded same-policy workers with fresh factory generation per task."""
import argparse
from concurrent.futures import ThreadPoolExecutor, wait, FIRST_COMPLETED
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

import torch

from full_factory_environment import FactoryRuntime
from full_factory_input_contracts import InputContractPolicy
from full_factory_input_learning import rollout, empty_gradients, accumulate_gradients
from full_factory_input_screen import process_rows, terminate_owned
from full_factory_on_policy import parameter_hash, gradient_norm
from full_factory_screen import sha
from random_factory_generation import generate, read, write


def check_sources(protocol):
    for name, expected in protocol['sources'].items():
        if sha(name) != expected:
            raise ValueError(f'random-collection.changed-source: {name}')


def tensor_hash(tensors):
    result = hashlib.sha256()
    for tensor in tensors:
        value = tensor.detach().cpu().contiguous().numpy()
        result.update(str((str(value.dtype), value.shape)).encode())
        result.update(value.tobytes())
    return result.hexdigest()


def fractional_score(row):
    return [Fraction(after - before, max(1, before)) for before, after
            in zip(row['initial_metrics']['score'], row['best_metrics']['score'])]


def fresh_group_gradients(model, episodes, scores, entropy_coefficient):
    if len(scores) < 2 or len(scores) != len(episodes) or any(
            len(score) != 3 or any(not isinstance(v, Fraction) for v in score) for score in scores):
        raise ValueError('random-collection.fractional-score-contract')
    ranks = [sum((score < other) - (score > other) for other in scores) / (len(scores) - 1)
             for score in scores]
    reward, entropy = empty_gradients(model), empty_gradients(model)
    for episode, rank in zip(episodes, ranks):
        accumulate_gradients(reward, episode['log_probability_gradients'], -rank / len(episodes))
        accumulate_gradients(entropy, episode['entropy_gradients'], -entropy_coefficient / len(episodes))
    for parameter, a, b in zip(model.parameters(), reward, entropy):
        parameter.grad = a + b
    return dict(returns=ranks, reward_gradient_norm=gradient_norm(reward), entropy_gradient_norm=gradient_norm(entropy),
        combined_gradient_norm=gradient_norm([p.grad for p in model.parameters()]), flat_returns=not any(ranks),
        reward_loss=-sum(a * e['mean_log_probability'] for a, e in zip(ranks, episodes)) / len(episodes),
        entropy_loss=-entropy_coefficient * sum(e['mean_entropy'] for e in episodes) / len(episodes))


def update(model, optimizer, results, config):
    accepted = [r for r in results if r['status'] == 'complete']
    if len(accepted) < 2:
        raise ValueError('random-collection.insufficient-valid-episodes')
    before = parameter_hash(model)
    gradients, scores = [], []
    for result in accepted:
        if result['policy_sha256'] != before or sha(result['gradients']) != result['gradients_sha256']:
            raise ValueError('random-collection.gradient-identity')
        payload = torch.load(result['gradients'], map_location='cpu', weights_only=False)
        if payload['policy_sha256'] != before or payload['input_sha256'] != result['input_sha256']:
            raise ValueError('random-collection.gradient-payload')
        for name in ('log_probability_gradients', 'entropy_gradients'):
            values = payload['gradients'][name]
            parameters = list(model.parameters())
            if len(values) != len(parameters) or any(v.shape != p.shape or v.dtype != p.dtype
                    or not torch.isfinite(v).all() for v, p in zip(values, parameters)):
                raise ValueError('random-collection.gradient-tensor-contract')
        if tensor_hash(payload['gradients']['log_probability_gradients'] + payload['gradients']['entropy_gradients']) != result['gradient_values_sha256']:
            raise ValueError('random-collection.gradient-values-identity')
        gradients.append(payload['gradients'])
        scores.append(fractional_score(result['row']))
    optimizer.zero_grad(set_to_none=True)
    accounting = fresh_group_gradients(model, gradients, scores, config['entropy_coefficient'])
    norm = torch.nn.utils.clip_grad_norm_(model.parameters(), config['gradient_clip'])
    if not torch.isfinite(norm):
        raise ValueError('random-collection.nonfinite-gradient')
    gradient_hash = tensor_hash([p.grad for p in model.parameters()])
    optimizer.step()
    return dict(**accounting, valid_episodes=len(accepted), policy_before_sha256=before,
        policy_after_sha256=parameter_hash(model), applied_gradient_sha256=gradient_hash,
        fractional_scores=[[[v.numerator, v.denominator] for v in score] for score in scores],
        pre_clip_gradient_norm=float(norm))


def worker(args):
    runtime, active = None, None
    def interrupted(signum, _):
        raise InterruptedError(f'random-collection.worker-interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    try:
        protocol = read(args.protocol)
        check_sources(protocol)
        spec = protocol['spec']
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        torch.use_deterministic_algorithms(True)
        model = InputContractPolicy(spec['policy']['depth'], spec['policy']['width'])
        runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
        loaded = None
        print(json.dumps(dict(kind='ready', pid=os.getpid())), flush=True)
        for line in sys.stdin:
            command = json.loads(line)
            if command.get('op') == 'stop':
                break
            active = Path(command['task'])
            task = read(active)
            began = time.monotonic()
            def guard():
                if time.monotonic() - began >= spec['limits']['task_seconds']:
                    raise TimeoutError(f'random-collection.task-budget: {active}')
            if task['policy_sha256'] != loaded:
                if sha(task['policy']) != task['policy_file_sha256']:
                    raise ValueError('random-collection.policy-file-identity')
                saved = torch.load(task['policy'], map_location='cpu', weights_only=False)
                model.load_state_dict(saved['policy'], strict=True)
                loaded = parameter_hash(model)
                if loaded != task['policy_sha256']:
                    raise ValueError('random-collection.policy-identity')
            case, generation = generate(runtime, spec, task['generation_seed'], active.parent / 'generated', guard)
            result = dict(input_sha256=sha(active), identity=task['identity'], generation=generation,
                          policy_sha256=loaded, pid=os.getpid())
            if case is None:
                result.update(status='generation-rejected')
            else:
                selection = dict(spec['selection'], horizon=task['horizon'])
                row, gradients = rollout(runtime, dict(config=selection), case, model,
                    'action-only', task['action_seed'], active.parent / 'episode', task['training'], guard)
                result.update(status='complete', row=row)
                if gradients is not None:
                    path = active.parent / 'gradients.pt'
                    torch.save(dict(policy_sha256=loaded, input_sha256=sha(active), gradients=gradients), path)
                    result.update(gradients=str(path), gradients_sha256=sha(path),
                        gradient_values_sha256=tensor_hash(gradients['log_probability_gradients'] + gradients['entropy_gradients']))
            result['seconds'] = time.monotonic() - began
            write(active.parent / 'result.json', result)
            print(json.dumps(dict(kind='result', path=str(active.parent / 'result.json'),
                                  sha256=sha(active.parent / 'result.json'))), flush=True)
            active = None
        check_sources(protocol)
    except BaseException as error:
        diagnostic = dict(status='incomplete', code='random-collection.worker-failed', severity='error',
                          stage='experience-collection', entity=str(active), message=str(error))
        if active is not None:
            write(active.parent / 'partial.json', diagnostic)
        print(json.dumps(dict(kind='error', diagnostic=diagnostic)), flush=True)
        raise
    finally:
        if runtime is not None:
            runtime.close()


class Pool:
    def __init__(self, protocol_path, workers, directory):
        self.protocol_path = Path(protocol_path).resolve()
        self.protocol = read(self.protocol_path)
        self.limits = self.protocol['spec']['limits']
        if type(workers) is not int or not 1 <= workers <= 64:
            raise ValueError('random-collection.worker-count')
        self.directory = Path(directory)
        self.directory.mkdir()
        self.children, self.logs = [], []
        self.executor = ThreadPoolExecutor(max_workers=workers)
        self.peak_rss, self.memory_samples, self.last_sample = 0, 0, 0
        began = time.monotonic()
        try:
            environment = dict(os.environ, OMP_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1', MKL_NUM_THREADS='1',
                               NUMEXPR_NUM_THREADS='1', PYTHONHASHSEED='0')
            for index in range(workers):
                log = open(self.directory / f'worker-{index}.log', 'w')
                self.logs.append(log)
                child = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), 'worker',
                    '--protocol', str(self.protocol_path)], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                    stderr=log, text=True, bufsize=1, start_new_session=True, env=environment)
                self.children.append(child)
            pending = {self.executor.submit(self.receive, i): i for i in range(workers)}
            deadline = began + self.limits['startup_seconds']
            while pending:
                self.guard(deadline)
                done, _ = wait(pending, timeout=.1, return_when=FIRST_COMPLETED)
                for future in done:
                    message = future.result()
                    if message.get('kind') != 'ready':
                        raise ValueError(f'random-collection.startup: {message}')
                    del pending[future]
            self.startup_seconds = time.monotonic() - began
            write(self.directory / 'pool.json', dict(workers=workers, pids=[c.pid for c in self.children],
                startup_seconds=self.startup_seconds, protocol_sha256=sha(self.protocol_path)))
        except BaseException:
            self.close()
            raise

    def receive(self, slot):
        line = self.children[slot].stdout.readline()
        if not line:
            raise RuntimeError(f'random-collection.worker-eof: {self.children[slot].pid}')
        message = json.loads(line)
        if message.get('kind') == 'error':
            raise RuntimeError(f'random-collection.worker-error: {message["diagnostic"]}')
        return message

    def exchange(self, slot, task):
        child = self.children[slot]
        child.stdin.write(json.dumps(dict(task=str(task))) + '\n')
        child.stdin.flush()
        message = self.receive(slot)
        if message.get('kind') != 'result' or sha(message['path']) != message['sha256']:
            raise ValueError(f'random-collection.result-envelope: {task}')
        result = read(message['path'])
        source = read(task)
        if (result['input_sha256'] != sha(task) or result['identity'] != source['identity']
                or result['policy_sha256'] != source['policy_sha256']):
            raise ValueError(f'random-collection.result-identity: {task}')
        if result['status'] == 'complete' and (not result['row']['budget_complete']
                or result['row']['selected'] != source['horizon']):
            raise ValueError(f'random-collection.incomplete-episode: {task}')
        return dict(result, result_path=message['path'], result_sha256=message['sha256'])

    def guard(self, deadline):
        now = time.monotonic()
        if now >= deadline:
            raise TimeoutError('random-collection.collection-budget')
        if now - self.last_sample >= .5:
            groups = {child.pid for child in self.children}
            rss = sum(row[3] for row in process_rows() if row[0] == os.getpid() or row[2] in groups) * 1024
            self.peak_rss = max(self.peak_rss, rss)
            self.last_sample = now
            self.memory_samples += 1
            if rss > self.limits['rss_limit_bytes']:
                raise MemoryError(f'random-collection.rss-budget: {rss}')

    def collect(self, tasks, deadline=None):
        began = time.monotonic()
        pending, results, completion_order = {}, {}, []
        next_task = 0
        deadline = min(deadline or float('inf'), began + self.limits['batch_seconds'])
        for slot in range(min(len(tasks), len(self.children))):
            pending[self.executor.submit(self.exchange, slot, tasks[next_task])] = (slot, next_task, time.monotonic())
            next_task += 1
        try:
            while pending:
                self.guard(deadline)
                overdue = [index for _, index, assigned in pending.values()
                           if time.monotonic() - assigned > self.limits['task_seconds'] + 5]
                if overdue:
                    raise TimeoutError(f'random-collection.worker-watchdog: {overdue}')
                done, _ = wait(pending, timeout=.1, return_when=FIRST_COMPLETED)
                for future in done:
                    slot, index, _ = pending.pop(future)
                    results[index] = future.result()
                    completion_order.append(index)
                    if next_task < len(tasks):
                        pending[self.executor.submit(self.exchange, slot, tasks[next_task])] = (slot, next_task, time.monotonic())
                        next_task += 1
            return [results[i] for i in range(len(tasks))], dict(seconds=time.monotonic() - began,
                completion_order=completion_order, peak_sampled_rss_bytes=self.peak_rss,
                memory_samples=self.memory_samples, startup_seconds=self.startup_seconds)
        except BaseException as error:
            write(self.directory / 'collection-partial.json', dict(status='incomplete',
                code='random-collection.batch-failed', severity='error', stage='experience-collection',
                entity=str(self.directory), message=str(error), completed=results))
            raise

    def close(self):
        try:
            terminate_owned(self.children)
        finally:
            self.executor.shutdown(wait=True, cancel_futures=True)
            for child in self.children:
                if child.stdin is not None:
                    try:
                        child.stdin.close()
                    except BrokenPipeError:
                        pass
                if child.stdout is not None:
                    child.stdout.close()
            for log in self.logs:
                log.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    execute = sub.add_parser('worker')
    execute.add_argument('--protocol', type=Path, required=True)
    worker(parser.parse_args())
