"""Frozen-weight episode gradients and physical rollouts for input-contract trials."""
from collections import Counter
from pathlib import Path
import time

import numpy as np
import torch

from full_factory_environment import FactoryEpisode, digest
from full_factory_input_contracts import observation, observation_hash, selected_action_token, history_record, selection_mask
from full_factory_on_policy import parameter_hash, probability_hash, gradient_norm, relative_returns
from full_factory_screen import save_gzip, write
from full_factory_selection import SelectionMemory, distribution, physical_key


def empty_gradients(model):
    return [torch.zeros_like(p) for p in model.parameters()]


def accumulate_gradients(destination, values, scale=1.0):
    for target, value in zip(destination, values):
        if value is not None:
            target.add_(value.detach(), alpha=scale)


def group_gradients(model, episodes, scores, entropy_coefficient):
    """Episodes contain gradients of mean log-probability and mean entropy."""
    if len(episodes) != len(scores) or len(episodes) < 2:
        raise ValueError('input-learning.group-shape')
    advantages = relative_returns(scores)
    reward, entropy = empty_gradients(model), empty_gradients(model)
    for episode, advantage in zip(episodes, advantages):
        accumulate_gradients(reward, episode['log_probability_gradients'], -advantage / len(episodes))
        accumulate_gradients(entropy, episode['entropy_gradients'], -entropy_coefficient / len(episodes))
    for parameter, a, b in zip(model.parameters(), reward, entropy):
        parameter.grad = a + b
    return dict(returns=advantages, reward_gradient_norm=gradient_norm(reward), entropy_gradient_norm=gradient_norm(entropy),
                combined_gradient_norm=gradient_norm([p.grad for p in model.parameters()]), flat_returns=not any(advantages),
                reward_loss=-sum(a * e['mean_log_probability'] for a, e in zip(advantages, episodes)) / len(episodes),
                entropy_loss=-entropy_coefficient * sum(e['mean_entropy'] for e in episodes) / len(episodes))


def rollout(runtime, protocol, case, model, candidate, seed, directory, training, guard):
    began = time.perf_counter()
    directory = Path(directory)
    directory.mkdir()
    config = protocol['config']
    before_hash = parameter_hash(model)
    env = None
    episode_gradients = dict(log_probability_gradients=empty_gradients(model), entropy_gradients=empty_gradients(model),
                             mean_log_probability=0.0, mean_entropy=0.0) if training else None
    try:
        env = FactoryEpisode(runtime, case, config['horizon'], config['episode_seconds'], config['route_work'])
        env.started = began
        memory, history = SelectionMemory(env.state['current']), []
        rng = torch.Generator().manual_seed(seed)
        while not env.done:
            guard()
            ordinal = env.selected
            description = env.describe()
            keys, allowed, exhausted = selection_mask(env.state, description, memory, candidate, config['memory'])
            obs = observation(env.state, description, 1 - ordinal / config['horizon'], history, candidate, runtime)
            obs_hash = observation_hash(obs)
            before_score = env.state['current_metrics']['score']
            before_physical = physical_key(env.state['current'])
            before_witness = digest(env.state['current'])
            with torch.set_grad_enabled(training):
                dist = distribution(model(obs) / config['temperature'], description['actions'], allowed, config['balanced'])
                index = int(torch.multinomial(dist.probs.detach(), 1, generator=rng))
                logp, entropy = dist.log_prob(torch.tensor(index)), dist.entropy()
                sampled_logp, sampled_entropy = float(logp.detach()), float(entropy.detach())
                sampled_hash = probability_hash(dist.probs)
                if not np.isfinite(sampled_logp) or not np.isfinite(sampled_entropy):
                    raise ValueError('input-learning.nonfinite-distribution')
                if training:
                    parameters = list(model.parameters())
                    values = torch.autograd.grad(logp, parameters, retain_graph=True, allow_unused=True)
                    accumulate_gradients(episode_gradients['log_probability_gradients'], values, 1 / config['horizon'])
                    values = torch.autograd.grad(entropy, parameters, allow_unused=True)
                    accumulate_gradients(episode_gradients['entropy_gradients'], values, 1 / config['horizon'])
                    episode_gradients['mean_log_probability'] += sampled_logp / config['horizon']
                    episode_gradients['mean_entropy'] += sampled_entropy / config['horizon']
            guard()
            if env.done:
                break
            action_token = selected_action_token(obs, index)
            step = env.step(description['actions'][index])
            after_physical = physical_key(env.state['current'])
            unchanged, revisited = memory.record(before_physical, after_physical, keys[index])
            step.update(vocabulary_index=index, observation_sha256=obs_hash, allowed_sha256=digest(allowed),
                allowed_count=sum(allowed), memory_exhausted=exhausted, old_log_probability=sampled_logp,
                sampling_entropy=sampled_entropy, probabilities_sha256=sampled_hash,
                current_before_sha256=before_witness, current_sha256=digest(env.state['current']),
                best_sha256=digest(env.state['best']), physical_before=before_physical, physical_after=after_physical,
                physical_unchanged=unchanged, physical_revisited=revisited, action_key=keys[index])
            history.append(history_record(action_token, before_score, step, env.state['initial_metrics']['score']))
            history = history[-16:]
        summary = env.summary()
        summary.update(schema_version=1, candidate=candidate, training=training, sample_seed=seed, policy_sha256=before_hash,
            observation_schema=obs['schema'] if env.selected else None, budget_complete=env.selected == config['horizon'] and
                time.perf_counter() - began < config['episode_seconds'] and not any(t['status'] == 'late' for t in env.trace))
        save_gzip(directory / 'episode.json.gz', summary)
        save_gzip(directory / 'best.json.gz', env.state['best'])
        if not summary['budget_complete']:
            raise TimeoutError(f'input-learning.episode-budget: {directory}')
        if parameter_hash(model) != before_hash:
            raise ValueError('input-learning.behavior-mutated')
        row = dict(case=case['id'], instances=case['instances'], candidate=candidate, sample_seed=seed,
            selected=env.selected, directory=str(directory.resolve()), training=training,
            initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'],
            kinds=dict(Counter(t['action']['kind'] for t in env.trace)), statuses=dict(Counter(t['status'] for t in env.trace)),
            improvements=dict(Counter(t['action']['kind'] for t in env.trace if t['status'] == 'improved')),
            route_expansions=sum(t['route_expansions'] for t in env.trace), seconds=time.perf_counter() - began,
            policy_sha256=before_hash, budget_complete=True)
        return row, episode_gradients
    except BaseException as error:
        if env is not None:
            save_gzip(directory / 'episode-partial.json.gz', env.summary())
            save_gzip(directory / 'best.json.gz', env.state['best'])
        write(directory / 'partial.json', dict(status='incomplete', code='input-learning.rollout-failed', severity='error',
            stage='rollout', entity=case['id'], message=str(error), optimizer_updates=0))
        raise
