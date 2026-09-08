"""Bounded waves of independent native episodes sharing policy inference only."""

import json

import torch

from policy import batch


def batch_size(config):
    size = config.get('evaluation_batch_size', 16)
    if type(size) is not int or size < 1:
        raise ValueError('evaluation.batch_size: evaluation_batch_size must be a positive integer')
    return size


@torch.no_grad()
def episodes(actor, runtime, count, horizon, seed_start, size, prefix=None):
    if any(type(value) is not int for value in (count, horizon, seed_start, size)) or min(count, horizon, size) < 1 or seed_start < 0:
        raise ValueError('evaluation.budget: invalid episode, horizon, seed or batch budget')
    device = next(actor.parameters()).device
    for first in range(0, count, size):
        seeds = list(range(seed_start+first, seed_start+min(first+size, count)))
        envs = [runtime.new_engine() for _ in seeds]
        generators = [torch.Generator(device='cpu').manual_seed(seed) for seed in seeds]
        totals = [0.0 for _ in seeds]
        if prefix is not None:
            encoded = json.dumps(prefix)
            for env in envs:
                env.reset_with_prefix_json(encoded)
        for _ in range(horizon):
            observed = [env.observe() for env in envs]
            data = {key: value.to(device) for key, value in batch([row[0] for row in observed]).items()}
            logits, _ = actor(data)
            log_probs = logits.log_softmax(-1).cpu()
            for index, (env, (observation, handle), generator) in enumerate(zip(envs, observed, generators)):
                # Zero-probability padding still consumes RNG draws in multinomial.
                probabilities = log_probs[index, :len(observation['candidates'])].exp()
                action = torch.multinomial(probabilities, 1, generator=generator).item()
                reward, _, _ = env.step(handle, action)
                totals[index] += reward
        for seed, env, total in zip(seeds, envs, totals):
            yield seed, env, total
