"""Audit native direct-reward trajectories, causal rewards and checkpoint transitions."""
from copy import deepcopy
import math
from pathlib import Path

import torch

from direct_area_reward import checked_payload, payload_hash, require, reward_sequence, update
from full_factory_area_learning import rollout
from full_factory_environment import digest
from full_factory_large_learning import unpack
from full_factory_on_policy import parameter_hash
from random_factory_audit import optimizer_hash, verify_native_result
from random_factory_generation import read
from random_factory_pilot import new_model


def physics_signature(result):
    generation = result['generation']
    draw = {k: v for k, v in generation['draw'].items() if k != 'source'}
    value = dict(status=result['status'], draw=draw, policy=result['policy_sha256'])
    if result['status'] == 'complete':
        trace = unpack(Path(result['row']['directory']) / 'episode.json.gz')['trace']
        keys = ('action', 'status', 'observation_sha256', 'allowed_sha256', 'probabilities_sha256',
                'old_log_probability', 'current_before_sha256', 'current_sha256', 'best_sha256',
                'current_score', 'best_score')
        value.update(request=generation['request_sha256'], initial=generation['initial_sha256'],
                     geometry=generation['geometry_sha256'], trace=[{k: s[k] for k in keys} for s in trace])
    return digest(value)


def verify_result(runtime, result, scratch):
    verify_native_result(runtime, result, scratch)
    if result['status'] != 'complete':
        return
    summary = unpack(Path(result['row']['directory']) / 'episode.json.gz')
    trace = summary['trace']
    initial = result['row']['initial_metrics']['score'][0]
    rewards = reward_sequence(initial, [initial] + [s['best_score'][0] for s in trace])
    remaining = math.fsum(rewards)
    previous = initial
    for step, reward in zip(trace, rewards):
        require(step['reward_denominator'] == initial and step['best_area_before'] == previous and
                step['reward_numerator'] == previous - step['best_score'][0], 'trace reward integers')
        require(step['reward'] == reward and math.isclose(step['reward_to_go'], remaining, abs_tol=1e-12),
                'trace reward and causal return')
        previous = step['best_score'][0]
        remaining -= reward
    require(math.isclose(summary['reward_sum'], math.fsum(rewards), abs_tol=1e-12), 'episode reward sum')
    task = read(Path(result['result_path']).with_name('task.json'))
    if task['training']:
        payload = checked_payload(result)
        require(payload['rewards'] == rewards, 'payload rewards match native trace')


def native_batch(runtime, results, directory):
    directory.mkdir()
    for result in results:
        verify_result(runtime, result, directory)
    (directory / 'best.json').unlink(missing_ok=True)
    return dict(results=len(results), admitted=sum(r['status'] == 'complete' for r in results),
                native_initial_and_best=True, rewards_and_payloads=True)


def transition(model, optimizer, results, config):
    other = deepcopy(model)
    other_optimizer = torch.optim.Adam(other.parameters(), lr=config['learning_rate'])
    other_optimizer.load_state_dict(deepcopy(optimizer.state_dict()))
    first = update(model, optimizer, results, config)
    second = update(other, other_optimizer, results, config)
    require(first == second and parameter_hash(model) == parameter_hash(other) and
            optimizer_hash(optimizer) == optimizer_hash(other_optimizer), 'optimizer reconstruction')
    return dict(**first, optimizer_sha256=optimizer_hash(optimizer), transition_reconstructed=True)


def verify_checkpoint(path, model, optimizer):
    saved = torch.load(path, map_location='cpu', weights_only=False)
    other = deepcopy(model)
    other.load_state_dict(saved['policy'], strict=True)
    other_optimizer = torch.optim.Adam(other.parameters(), lr=optimizer.param_groups[0]['lr'])
    other_optimizer.load_state_dict(saved['optimizer'])
    require(parameter_hash(other) == parameter_hash(model) == saved['policy_sha256'] and
            optimizer_hash(other_optimizer) == optimizer_hash(optimizer) and
            torch.equal(saved['rng'], torch.get_rng_state()), 'checkpoint weights/Adam/RNG reopen')


def replay(runtime, protocol, result, directory, guard):
    rng_before = torch.get_rng_state()
    try:
        task = read(Path(result['result_path']).with_name('task.json'))
        saved = torch.load(task['policy'], map_location='cpu', weights_only=False)
        model = new_model(protocol['spec'], 0)
        model.load_state_dict(saved['policy'], strict=True)
        require(parameter_hash(model) == task['policy_sha256'], 'replay policy identity')
        selection = dict(protocol['spec']['selection'], horizon=task['horizon'])
        row, gradients = rollout(runtime, dict(config=selection), result['generation']['case'], model, 'action-only',
                                task['action_seed'], directory, task['training'], guard)
        require(physics_signature(dict(result, row=row)) == physics_signature(result), 'replayed physical trajectory')
        if gradients is not None:
            require(payload_hash(gradients) == result['gradient_values_sha256'], 'replayed reward gradients')
        return dict(selected=row['selected'], native_and_sampling_match=True, reward_gradients_match=gradients is not None)
    finally:
        torch.set_rng_state(rng_before)
