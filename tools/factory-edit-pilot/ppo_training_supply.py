"""One-ahead CPU preparation with deterministic order and bounded outstanding work."""
from concurrent.futures import TimeoutError
import time

from ppo_training_batch import prepare


def prepare_cpu(steps, advantages, targets, config, width, microbatch):
    chunks = []
    for offset in range(0, len(steps), microbatch):
        stop = min(len(steps), offset + microbatch)
        chunks.append(dict(batch=prepare(steps[offset:stop], config, width, 'cpu'),
                           advantages=advantages[offset:stop], targets=targets[offset:stop]))
    return chunks


def prepared_batches(executor, steps, advantages, targets, config, width, microbatch, order,
                     minibatch_size, guard):
    """Only the current minibatch and one future CPU minibatch may be resident."""
    def submit(offset):
        indices = order[offset:offset + minibatch_size]

        def work():
            began = time.perf_counter()
            chunks = prepare_cpu([steps[i] for i in indices], advantages[indices], targets[indices],
                                 config, width, microbatch)
            return indices, chunks, time.perf_counter() - began

        return executor.submit(work)

    future = submit(0)
    try:
        for offset in range(0, len(order), minibatch_size):
            began = time.perf_counter()
            while True:
                guard()
                try:
                    indices, chunks, worker_seconds = future.result(timeout=.1)
                    break
                except TimeoutError:
                    if future.done():
                        raise
            wait_seconds = time.perf_counter() - began
            future = submit(offset + minibatch_size) if offset + minibatch_size < len(order) else None
            yield indices, chunks, wait_seconds, worker_seconds
    finally:
        if future is not None:
            future.cancel()
