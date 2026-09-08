"""Explicit native curriculum starts and complete-episode multi-environment collection."""

from concurrent.futures import ThreadPoolExecutor
import json
import time

import torch

from native import snapshot
from policy import batch


FIELDS = ("observations", "actions", "log_probs", "values", "rewards", "dones")


def remaining_at(protocol, condition, commands):
    if protocol["conditions"][condition]["curriculum"]:
        for stage in protocol["curriculum"]:
            if commands < stage["until"]:
                return stage["remaining"]
    return 0


def prefixes(runtime):
    """Use only the first success; no rewarded prefix or target action enters PPO."""
    env = runtime.new_engine()
    path = []
    for command in runtime.case["witness"]:
        handle, _ = env.candidates()
        domain = json.loads(env.export_candidates_json(handle))
        if command not in domain:
            raise ValueError(f"curriculum.witness_domain: {runtime.path}: {command}")
        reward, done, outcome = env.step(handle, domain.index(command))
        path.append(command)
        if not outcome["applied"] or done:
            raise ValueError(f"curriculum.witness_step: {runtime.path}: {outcome}")
        if reward > 0:
            break
    else:
        raise ValueError(f"curriculum.witness_reward: {runtime.path}: no positive witness reward")
    if len(path) < 3:
        raise ValueError(f"curriculum.witness_short: {runtime.path}: three curriculum levels required")
    result = {0: []}
    for remaining in range(1, len(path) + 1):
        prefix = path[:-remaining]
        env.reset_with_prefix_json(json.dumps(prefix))
        state = snapshot(env)
        if state["steps"] != 0 or state["best_score"] != list(env.initial_score):
            raise RuntimeError("curriculum.start_accounting: native prefix admission returned inconsistent state")
        result[remaining] = prefix
    return result


@torch.no_grad()
def act_batch(model, observations, generators):
    device = next(model.parameters()).device
    inputs = {key: value.to(device) for key, value in batch(observations).items()}
    logits, values = model(inputs)
    log_probs = logits.log_softmax(-1).cpu()
    values = values.cpu().tolist()
    selected = [torch.multinomial(log_probs[i, :len(observations[i]['candidates'])].exp(), 1, generator=generator).item()
                for i, generator in enumerate(generators)]
    return [(action, log_probs[i, action].item(), values[i]) for i, action in enumerate(selected)]


class Collector:
    """Eight independently sampled episodes per update; immutable policy during collection."""

    def __init__(self, runtimes, config, streams, seed, parallel, threaded=True):
        self.keys = [list(runtimes)[i % len(runtimes)] for i in range(streams)]
        self.envs = [runtimes[key].new_engine() for key in self.keys]
        self.starts = {key: prefixes(runtime) for key, runtime in runtimes.items()}
        self.generators = [torch.Generator(device="cpu").manual_seed(seed + 1009 * (i + 1))
                           for i in range(streams)]
        self.horizon = config["horizon"]
        self.parallel = parallel
        self.pool = ThreadPoolExecutor(max_workers=streams) if parallel and threaded else None
        self.partial = None
        self.pending = None
        self.completed_steps = 0
        self.seconds = {key: 0.0 for key in ("setup", "observe", "inference", "step")}

    def close(self):
        if self.pool is not None:
            self.pool.shutdown(wait=True)

    def mapped(self, function, items):
        if self.pool is None:
            return list(map(function, items))
        futures = [self.pool.submit(function, item) for item in items]
        results, errors = [], []
        for future in futures:
            try:
                results.append(future.result())
            except BaseException as error:
                errors.append(error)
        if errors:
            raise errors[0]
        return results

    def collect(self, model, levels):
        if (len(levels) != len(self.envs)
                or any(type(level) is not int or level not in self.starts[key] for key, level in zip(self.keys, levels))):
            raise ValueError("collection.levels: one valid declared level is required for every stream")
        started = time.perf_counter()
        self.mapped(lambda pair: pair[0].reset_with_prefix_json(json.dumps(self.starts[pair[1]][pair[2]])),
                    list(zip(self.envs, self.keys, levels)))
        self.seconds["setup"] += time.perf_counter() - started
        rows = [{key: [] for key in FIELDS} for _ in self.envs]
        self.partial = {"schema_version": 2, "levels": list(levels), "streams": rows}
        self.pending = None
        self.completed_steps = 0
        groups = [list(range(len(self.envs)))] if self.parallel else [[i] for i in range(len(self.envs))]
        for indices in groups:
            for _ in range(self.horizon):
                clock = time.perf_counter()
                observed = self.mapped(lambda i: self.envs[i].observe(), indices)
                self.seconds["observe"] += time.perf_counter() - clock
                clock = time.perf_counter()
                if self.parallel:
                    actions = act_batch(model, [pair[0] for pair in observed], [self.generators[i] for i in indices])
                else:
                    actions = [model.act(observed[0][0], self.generators[indices[0]])]
                self.seconds["inference"] += time.perf_counter() - clock
                clock = time.perf_counter()
                self.pending = {"indices": indices, "observations": [pair[0] for pair in observed],
                                "actions": actions, "transitions": {}}

                def step(item):
                    result = self.envs[item[0]].step(item[1], item[2])
                    self.pending["transitions"][item[0]] = result
                    return result

                transitions = self.mapped(step,
                                          [(i, pair[1], action[0]) for i, pair, action in zip(indices, observed, actions)])
                self.seconds["step"] += time.perf_counter() - clock
                for i, pair, action, transition in zip(indices, observed, actions, transitions):
                    reward, done, _ = transition
                    for key, value in zip(FIELDS, (pair[0], *action, reward, done)):
                        rows[i][key].append(value)
                    self.completed_steps += 1
                self.pending = None
        rollout = {key: [item for row in rows for item in row[key]] for key in FIELDS}
        if any(row["dones"] != [False] * (self.horizon - 1) + [True] for row in rows):
            raise RuntimeError("collection.episode_boundary: each stream must contain one complete episode")
        return rollout, rows
