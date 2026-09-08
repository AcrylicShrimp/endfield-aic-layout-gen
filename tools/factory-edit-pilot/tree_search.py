"""Bounded hypothetical tree allocation over complete native action histories."""

from collections import Counter
import json
import math
import random
import sys
import time

import numpy as np
import torch

from native_training_comparison import policy_hash
from policy import batch


METHODS = ('random_rollouts', 'policy_rollouts', 'uct', 'puct')


class Tree:
    """A fixed array pool; no per-node geometry, snapshots, observations or hashes."""

    def __init__(self, capacity, max_bytes):
        required = 5*(sys.getsizeof(np.empty(0, dtype=np.int64))+capacity*8)
        if required > max_bytes:
            raise ValueError(f'tree.memory: {required} array bytes exceed allowance {max_bytes}')
        self.first = np.full(capacity, -1, dtype=np.int64)
        self.next = np.full(capacity, -1, dtype=np.int64)
        self.action = np.full(capacity, -1, dtype=np.int64)
        self.visits = np.zeros(capacity, dtype=np.int64)
        self.total = np.zeros(capacity, dtype=np.float64)
        self.bytes = sum(sys.getsizeof(array) for array in (self.first, self.next, self.action, self.visits, self.total))
        self.used, self.capacity = 1, capacity

    def children(self, node):
        result = {}
        child = int(self.first[node])
        while child != -1:
            result[int(self.action[child])] = child
            child = int(self.next[child])
        return result

    def add(self, parent, action):
        if self.used == self.capacity:
            return None
        child = self.used
        self.used += 1
        self.action[child] = action
        self.next[child] = self.first[parent]
        self.first[parent] = child
        return child

    def choose(self, node, count, priors, method, rng):
        children = self.children(node)
        if any(action >= count for action in children):
            raise ValueError('tree.domain: replayed history changed its candidate domain')
        unvisited = [action for action in range(count) if action not in children]
        if method == 'uct' and unvisited:
            action = rng.choice(unvisited)
        else:
            values = []
            for action in range(count):
                child = children.get(action)
                visits = 0 if child is None else int(self.visits[child])
                mean = 0 if not visits else float(self.total[child])/visits
                if method == 'uct':
                    bonus = math.sqrt(2)*math.sqrt(math.log(max(1, int(self.visits[node])))/visits)
                else:
                    bonus = 1.25*float(priors[action])*math.sqrt(max(1, int(self.visits[node])))/(1+visits)
                values.append(mean+bonus)
            best = max(values)
            action = rng.choice([index for index, value in enumerate(values) if value == best])
        return action, children.get(action)

    def backup(self, path, value):
        for node in path:
            self.visits[node] += 1
            self.total[node] += value


def search(runtime, method, actor, seed, transitions, max_seconds, max_nodes, max_tree_bytes):
    if (method not in METHODS or type(seed) is not int or seed < 0
            or any(type(value) is not int or value < 1 for value in (transitions, max_nodes, max_tree_bytes))
            or not isinstance(max_seconds, (int, float)) or not 0 < max_seconds < float('inf')):
        raise ValueError('tree.input: unknown method or invalid search budget')
    guided = method in ('policy_rollouts', 'puct')
    if guided and (actor is None or actor.training):
        raise ValueError('tree.policy: learned arms require a frozen evaluation policy')
    started = time.perf_counter()
    initial_engine, engine = runtime.new_engine(), runtime.new_engine()
    original = initial_engine.export_state_json()
    initial = initial_engine.initial_score
    best = json.loads(initial_engine.export_layout_json('best'))
    best_score, best_trace = initial, []
    tree = Tree(max_nodes, max_tree_bytes) if method in ('uct', 'puct') else None
    rng, generator = random.Random(seed), torch.Generator(device='cpu').manual_seed(seed)
    before_policy = policy_hash(actor.state_dict()) if guided else None
    counts, branching = Counter(), Counter()
    first_improvement = None
    reason = 'transition_budget'

    @torch.no_grad()
    def probabilities(handle):
        counts['observations'] += 1
        observation = engine.observe_candidates(handle)
        data = {key: value.to(next(actor.parameters()).device) for key, value in batch([observation]).items()}
        logits, _ = actor(data)
        counts['model_calls'] += 1
        value = logits[0].softmax(-1).cpu()
        if not torch.isfinite(value).all() or float(value.sum()) <= 0:
            raise ValueError('tree.policy: nonfinite or empty action distribution')
        return value

    stop = False
    while counts['simulated_commands'] < transitions and not stop:
        if time.perf_counter()-started >= max_seconds:
            reason = 'wall_budget'
            break
        engine.restore_state_json(original)
        counts['restores'] += 1
        counts['rollouts'] += 1
        path, node, rollout, done = [0], 0, tree is None, False
        while not done and counts['simulated_commands'] < transitions:
            if time.perf_counter()-started >= max_seconds:
                reason, stop = 'wall_budget', True
                break
            handle, count = engine.candidates()
            counts['candidate_calls'] += 1
            branching[count] += 1
            if not count:
                raise ValueError('tree.domain: active state has no legal candidates')
            probs = probabilities(handle) if guided else None
            if rollout:
                action = int(torch.multinomial(probs, 1, generator=generator)) if guided else rng.randrange(count)
            else:
                action, child = tree.choose(node, count, probs, method, rng)
                if child is None:
                    child = tree.add(node, action)
                    if child is None:
                        reason, stop = 'node_budget', True
                        break
                    rollout = True
                node = child
                path.append(node)
            _, done, _ = engine.step(handle, action)
            counts['simulated_commands'] += 1
            if engine.best_score < best_score:
                best_score = engine.best_score
                best = json.loads(engine.export_layout_json('best'))
                best_trace = json.loads(engine.trace_json())
                if first_improvement is None:
                    first_improvement = counts['simulated_commands']
            if time.perf_counter()-started >= max_seconds:
                reason, stop = 'wall_budget', True
                break
        if tree is not None:
            tree.backup(path, (initial[0]-engine.best_score[0])/initial[0])
        if done:
            counts['completed_rollouts'] += 1
    if initial_engine.export_state_json() != original:
        raise ValueError('tree.root_mutation: hypothetical work changed original engine')
    if guided and policy_hash(actor.state_dict()) != before_policy:
        raise ValueError('tree.policy_mutation: frozen parameters or normalization buffers changed')
    accepted = json.loads(initial_engine.check_layout_json(json.dumps(best)))
    if not accepted['valid'] or accepted['score'] != best_score:
        raise ValueError('tree.incumbent: retained layout failed native validation')
    return {'schema_version': 1, 'method': method, 'seed': seed, 'termination': reason,
            'initial_score': initial, 'best_score': best_score, 'best': best, 'best_trace': best_trace,
            'result_origin': 'hypothetical_exploration', 'executed_commands': 0,
            'simulated_commands': counts['simulated_commands'], 'counts': dict(counts),
            'tree_nodes': tree.used if tree else 0, 'tree_array_bytes': tree.bytes if tree else 0,
            'branching_histogram': dict(branching), 'first_improvement_transition': first_improvement,
            'native_validation': accepted, 'original_engine_unchanged': True,
            'policy_sha256': before_policy, 'elapsed_seconds': time.perf_counter()-started}
