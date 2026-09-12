"""One persistent native stream with bounded shared-array responses."""
import argparse
import os
from pathlib import Path
import socket
import time
import traceback

import numpy as np
import torch

from blueprint_edit_input import BlueprintActorCritic, prepare
from blueprint_evaluation_cohort import case_runtime
from blueprint_ppo_collect import finish_stream, step_stream
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_evaluate import evaluate_case
from blueprint_ppo_update import require
from ppo_harness_transport import Reader, receive, send

SCHEMA = 'blueprint-worker-message-v1'


def arrays(value, outgoing):
    if outgoing and isinstance(value, torch.Tensor):
        return value.detach().cpu().numpy()
    if not outgoing and isinstance(value, np.ndarray):
        return torch.from_numpy(value)
    if isinstance(value, dict):
        return {k: arrays(v, outgoing) for k, v in value.items()}
    if isinstance(value, list):
        return [arrays(v, outgoing) for v in value]
    if isinstance(value, tuple):
        return tuple(arrays(v, outgoing) for v in value)
    return value


class Worker:
    def __init__(self):
        self.stream = None
        self.samples = []
        self.before = self.observation = self.prepared = None
        self.model = None

    def execute(self, command):
        operation = command['operation']
        if operation == 'initialize':
            config = command['config']
            self.runtime = Runtime(config, command['library'])
            saved = command.get('saved')
            self.stream = Stream(self.runtime, command['index']) if saved is None else Stream.restore(self.runtime, saved)
            return dict(pid=os.getpid(), affinity=sorted(os.sched_getaffinity(0)))
        if operation == 'begin':
            require(self.before is None, 'execution worker already holds an unfinished chunk')
            self.observation = self.stream.observe()
            self.before = self.stream.snapshot()
            self.samples = []
            self.prepared = prepare(self.observation)
            return dict(input=self.prepared)
        if operation == 'step':
            require(self.before is not None and not self.observation['done'], 'execution step requires a live chunk')
            require(len(self.samples) < self.runtime.config['training']['chunk_actions'], 'execution chunk action overflow')
            sample, after = step_stream(self.stream, self.observation, self.prepared,
                command['index'], command['old_logp'], command['old_value'], command['entropy'], self.runtime.config)
            self.samples.append(sample)
            self.observation = after
            self.prepared = prepare(after)
            return dict(input=self.prepared)
        if operation == 'finish':
            require(len(self.samples) == self.runtime.config['training']['chunk_actions'], 'execution chunk action coverage')
            frontier, completed = finish_stream(self.stream, self.observation, self.prepared, command['value'], self.runtime.config)
            result = dict(native_before=self.before, samples=self.samples, frontier=frontier, completed=completed)
            self.before, self.samples = None, []
            return result
        if operation == 'snapshot':
            require(self.before is None, 'cannot publish a durable checkpoint inside a partial chunk')
            return dict(stream=self.stream.snapshot())
        if operation == 'model':
            require(self.before is None, 'cannot replace behavior during a chunk')
            if self.model is None:
                ppo = self.runtime.config['ppo']
                self.model = BlueprintActorCritic(ppo['width'], ppo.get('message_rounds', 2))
            self.model.load_state_dict(command['state'], strict=True)
            self.model.eval()
            return dict(loaded=True)
        if operation == 'evaluate':
            require(self.before is None and self.model is not None, 'evaluation requires idle training and loaded weights')
            selected = case_runtime(self.runtime, command['case'])
            case = command['case']
            output = Path(command['output'])
            output.mkdir(parents=True, exist_ok=True)
            deadline = command['deadline']
            def guard():
                require(time.monotonic() < deadline, 'replay-balance.resource-exhausted: evaluation deadline')
            row = evaluate_case(selected, self.model, command['seed'], command['update'],
                dict(name=f'factory-{case["generation_seed"]}', prefix=[]), command.get('sample', 0),
                command['evaluation'], output, guard, command['action_seed'], case['initial_metrics']['score'])
            row.update(generation_seed=case['generation_seed'], action_seed=command['action_seed'],
                request_sha256=case['files']['request']['sha256'], initial_sha256=case['files']['initial']['sha256'])
            return dict(row=row)
        if operation == 'continuation':
            from blueprint_replay_anchors import continuation
            require(self.before is None and self.model is not None, 'continuation requires idle training and loaded weights')
            return dict(row=continuation(self.runtime, self.model, command))
        raise ValueError(f'unknown blueprint worker operation: {operation}')

    def preserve(self, directory, error):
        # An uncatchable process kill leaves the last coordinator checkpoint as
        # the durable boundary. Catchable failures retain the private partial too.
        directory.mkdir(parents=True, exist_ok=True)
        torch.save(dict(schema='blueprint-worker-partial-v1', error=str(error),
            stream=self.stream.snapshot() if self.stream is not None else None,
            native_before=self.before, samples=self.samples), directory / f'worker-{os.getpid()}.pt')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fd', type=int, required=True)
    parser.add_argument('--core', type=int, required=True)
    parser.add_argument('--message-bytes', type=int, required=True)
    parser.add_argument('--failure-directory', type=Path, required=True)
    args = parser.parse_args()
    os.sched_setaffinity(0, {args.core})
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    stream = socket.socket(fileno=args.fd)
    reader, worker = Reader(args.message_bytes), Worker()
    try:
        while True:
            data, descriptors = receive(stream)
            if not data:
                raise EOFError('coordinator disconnected')
            for wire in reader.feed(data, descriptors):
                command = arrays(wire, False)
                if command['operation'] == 'close':
                    return
                began = time.perf_counter()
                started_ns, cpu_before = time.monotonic_ns(), time.process_time()
                require(command['schema'] == SCHEMA, 'worker command schema')
                result = worker.execute(command)
                send(arrays(dict(schema=SCHEMA, request_id=command['request_id'], worker_index=worker.stream.index,
                    ok=True, result=result, seconds=time.perf_counter() - began,
                    interval_ns=(started_ns, time.monotonic_ns()), cpu_seconds=time.process_time() - cpu_before), True), stream)
    except BaseException as error:
        worker.preserve(args.failure_directory, error)
        try:
            send(dict(ok=False, error=str(error), traceback=traceback.format_exc()), stream)
        except (OSError, EOFError):
            pass
        raise
    finally:
        reader.close()
        stream.close()


if __name__ == '__main__':
    main()
