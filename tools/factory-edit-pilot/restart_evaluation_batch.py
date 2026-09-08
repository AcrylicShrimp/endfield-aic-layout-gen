"""Independent restarted episodes share inference while retaining each sampling stream."""

from collections import Counter
import json

import torch

from native import check, layout, trace
from policy import batch


@torch.no_grad()
def restarted(actor, runtime, count, horizon, total_commands, seed_start, size):
    if (any(type(v) is not int for v in (count, horizon, total_commands, seed_start, size))
            or min(count, horizon, total_commands, size) < 1 or seed_start < 0 or total_commands % horizon):
        raise ValueError('restart.batch_budget: invalid episode, attempt, command, seed or wave budget')
    device = next(actor.parameters()).device
    for first in range(0, count, size):
        seeds = range(seed_start+first, seed_start+min(first+size, count))
        engines = [runtime.new_engine() for _ in seeds]
        generators = [torch.Generator(device='cpu').manual_seed(seed) for seed in seeds]
        results = [{'seed': seed, 'best': None, 'best_trace': None, 'best_score': None,
                    'attempts': [], 'counts': Counter()} for seed in seeds]
        for attempt in range(total_commands//horizon):
            for engine in engines:
                engine.reset()
            first_gains = [None]*len(engines)
            for step in range(horizon):
                observed = [engine.observe() for engine in engines]
                data = {key: value.to(device) for key, value in batch([row[0] for row in observed]).items()}
                logits, _ = actor(data)
                probabilities = logits.log_softmax(-1).exp().cpu()
                for i, (engine, (observation, handle), generator) in enumerate(zip(engines, observed, generators)):
                    action = torch.multinomial(probabilities[i, :len(observation['candidates'])], 1, generator=generator).item()
                    reward, done, _ = engine.step(handle, action)
                    if done != (step+1 == horizon):
                        raise ValueError('restart.batch_horizon: native horizon differs from attempt budget')
                    if reward > 0 and first_gains[i] is None:
                        first_gains[i] = step+1
            for i, (engine, result) in enumerate(zip(engines, results)):
                candidate = layout(engine, 'best')
                accepted = check(engine, candidate)
                counts = json.loads(engine.episode_json())['counts']
                if not accepted['valid'] or counts['commands'] != horizon:
                    raise ValueError('restart.batch_attempt: invalid incumbent or attempt accounting')
                result['counts'].update(counts)
                result['attempts'].append({'attempt': attempt, 'score': accepted['score'],
                                          'first_improvement_step': first_gains[i], 'counts': counts})
                if result['best_score'] is None or accepted['score'] < result['best_score']:
                    result.update(best=candidate, best_trace=trace(engine), best_score=accepted['score'])
        yield from results
