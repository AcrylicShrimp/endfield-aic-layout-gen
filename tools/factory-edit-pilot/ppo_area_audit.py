"""Native experience audits and exact actor/critic optimizer reconstruction."""
from copy import deepcopy
import math
from pathlib import Path

import torch

from direct_area_audit import physics_signature
from direct_area_reward import reward_sequence
from ppo_reward import SCHEMA as SHAPED_REWARD_SCHEMA, validate_episode
from full_factory_environment import digest
from full_factory_input_contracts import observation_hash
from full_factory_large_learning import unpack
from full_factory_on_policy import parameter_hash
from full_factory_ppo_learning import rollout
from full_factory_screen import sha
from ppo_area_model import SCHEMA, models, require
from ppo_area_update import load_experience, update, sha_bytes
from random_factory_audit import optimizer_hash, verify_native_result
from random_factory_generation import read


def verify_result(runtime, result, scratch):
    verify_native_result(runtime, result, scratch)
    if result['status'] != 'complete':
        return
    summary = unpack(Path(result['row']['directory']) / 'episode.json.gz')
    trace = summary['trace']
    initial = result['row']['initial_metrics']['score'][0]
    rewards = reward_sequence(initial, [initial] + [step['best_score'][0] for step in trace])
    if summary.get('reward_schema') == SHAPED_REWARD_SCHEMA:
        validate_episode(trace, summary, summary['reward_config'])
        validate_episode(trace, result['row'], summary['reward_config'])
        require(all(step['area_reward'] == area for step, area in zip(trace, rewards)), 'native area component')
        rewards = [step['reward'] for step in trace]
    task = read(Path(result['result_path']).with_name('task.json'))
    require(task['critic_sha256'] == result['critic_sha256'], 'task critic identity')
    remaining = math.fsum(rewards)
    for step, reward in zip(trace, rewards):
        require(step['reward'] == reward and math.isclose(step['reward_to_go'], remaining, abs_tol=1e-12), 'native direct rewards')
        remaining -= reward
    if task['training']:
        experience = load_experience(result)['steps']
        require(len(experience) == len(trace), 'experience and trace length')
        for saved, actual, reward in zip(experience, trace, rewards):
            require(saved['kinds'][saved['index']] == actual['action']['kind'] and
                    digest(saved['allowed']) == actual['allowed_sha256'], 'saved candidate kind and mask')
            require(saved['index'] == actual['vocabulary_index'] and saved['reward'] == reward and
                    saved['old_log_probability'] == actual['old_log_probability'] and saved['old_value'] == actual['old_value'] and
                    saved['probabilities_sha256'] == actual['probabilities_sha256'] and
                    saved['observation_sha256'] == actual['observation_sha256'] and
                    saved['critic_observation_sha256'] == actual['critic_observation_sha256'], 'experience matches native trace')
            require(observation_hash(saved['actor_observation']) == saved['observation_sha256'] and
                    observation_hash(saved['critic_observation']) == saved['critic_observation_sha256'], 'saved observation bytes')


def native_batch(runtime, results, directory):
    directory.mkdir()
    for result in results:
        verify_result(runtime, result, directory)
    (directory / 'best.json').unlink(missing_ok=True)
    return dict(results=len(results), admitted=sum(r['status'] == 'complete' for r in results),
                native_initial_and_best=True, experience_and_rewards=True)


def transition(actor, critic, actor_optimizer, critic_optimizer, rng, results, config, guard):
    other_actor, other_critic = deepcopy(actor), deepcopy(critic)
    other_aopt, other_copt = (torch.optim.Adam(m.parameters(), lr=config['learning_rate']) for m in (other_actor, other_critic))
    other_aopt.load_state_dict(deepcopy(actor_optimizer.state_dict()))
    other_copt.load_state_dict(deepcopy(critic_optimizer.state_dict()))
    other_rng = torch.Generator().set_state(rng.get_state())
    first = update(actor, critic, actor_optimizer, critic_optimizer, rng, results, config, guard)
    second = update(other_actor, other_critic, other_aopt, other_copt, other_rng, results, config, guard)
    require(first == second, 'independently reconstructed PPO actor/critic updates')
    return dict(**first, transition_reconstructed=True)


def verify_checkpoint(path, actor, critic, actor_optimizer, critic_optimizer, rng):
    saved = torch.load(path, map_location='cpu', weights_only=False)
    require(saved['schema'] == SCHEMA, 'PPO checkpoint schema')
    other_a, other_c = deepcopy(actor), deepcopy(critic)
    other_a.load_state_dict(saved['policy'], strict=True)
    other_c.load_state_dict(saved['critic'], strict=True)
    ao, co = (torch.optim.Adam(m.parameters(), lr=actor_optimizer.param_groups[0]['lr']) for m in (other_a, other_c))
    ao.load_state_dict(saved['actor_optimizer'])
    co.load_state_dict(saved['critic_optimizer'])
    require(parameter_hash(other_a) == parameter_hash(actor) == saved['policy_sha256'] and
            parameter_hash(other_c) == parameter_hash(critic) == saved['critic_sha256'], 'actor/critic checkpoint weights')
    require(optimizer_hash(ao) == optimizer_hash(actor_optimizer) and optimizer_hash(co) == optimizer_hash(critic_optimizer),
            'actor/critic checkpoint optimizer states')
    require(torch.equal(saved['shuffle_rng'], rng.get_state()) and torch.equal(saved['rng'], torch.get_rng_state()),
            'checkpoint shuffle and torch RNG')


def replay(runtime, protocol, result, directory, guard):
    rng_before = torch.get_rng_state()
    try:
        task = read(Path(result['result_path']).with_name('task.json'))
        saved = torch.load(task['policy'], map_location='cpu', weights_only=False)
        actor, critic = models(protocol['spec'], 0)
        actor.load_state_dict(saved['policy'], strict=True)
        critic.load_state_dict(saved['critic'], strict=True)
        selection = dict(protocol['spec']['selection'], horizon=task['horizon'])
        row, payload = rollout(runtime, dict(config=selection), result['generation']['case'], actor, critic, 'action-only',
                               task['action_seed'], directory, task['training'], guard)
        require(physics_signature(dict(result, row=row)) == physics_signature(result), 'PPO trajectory replay')
        if payload is not None:
            stored = load_experience(result)
            require(len(payload['steps']) == len(stored['steps']), 'replayed experience length')
            for a, b in zip(payload['steps'], stored['steps']):
                require(observation_hash(a) == observation_hash(b), 'replayed actor/critic experience')
        return dict(selected=row['selected'], physical_and_sampling_match=True, critic_observations_match=payload is not None)
    finally:
        torch.set_rng_state(rng_before)
