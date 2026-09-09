"""Fresh on-policy actor/critic trajectories with immutable pre-action observations."""
from collections import Counter
from pathlib import Path
import time

import numpy as np
import torch

from full_factory_environment import FactoryEpisode, digest
from full_factory_input_contracts import observation, observation_hash, selected_action_token, history_record, selection_mask
from full_factory_on_policy import parameter_hash, probability_hash
from direct_area_reward import reward_sequence, SCHEMA
from ppo_area_model import SCHEMA as PPO_SCHEMA
from ppo_area_update import save_experience
from full_factory_screen import save_gzip, write
from full_factory_selection import SelectionMemory, distribution, physical_key


def rollout(runtime, protocol, case, model, critic, candidate, seed, directory, training, guard):
    began = time.perf_counter()
    directory = Path(directory)
    directory.mkdir()
    config = protocol['config']
    before_hash = parameter_hash(model)
    env = None
    experience = []
    critic_hash = parameter_hash(critic)
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
            before_best_area = env.state['best_metrics']['score'][0]
            before_physical = physical_key(env.state['current'])
            before_witness = digest(env.state['current'])
            with torch.no_grad():
                dist = distribution(model(obs) / config['temperature'], description['actions'], allowed, config['balanced'])
                index = int(torch.multinomial(dist.probs.detach(), 1, generator=rng))
                logp, entropy = dist.log_prob(torch.tensor(index)), dist.entropy()
                sampled_logp, sampled_entropy = float(logp.detach()), float(entropy.detach())
                sampled_hash = probability_hash(dist.probs)
                if not np.isfinite(sampled_logp) or not np.isfinite(sampled_entropy):
                    raise ValueError('ppo-area.nonfinite-distribution')
                critic_obs = observation(env.state, description, 1 - ordinal / config['horizon'], history,
                                         'current-best-layout', runtime) if training else None
                old_value = float(critic(critic_obs, allowed)) if training else None
                if training and not np.isfinite(old_value):
                    raise ValueError('ppo-area.behavior-mismatch: nonfinite value')
            guard()
            if env.done:
                break
            action_token = selected_action_token(obs, index)
            step = env.step(description['actions'][index])
            after_best_area = env.state['best_metrics']['score'][0]
            reward = reward_sequence(env.state['initial_metrics']['score'][0], [before_best_area, after_best_area])[0]
            step.update(best_area_before=before_best_area, reward_numerator=before_best_area - after_best_area,
                        reward_denominator=env.state['initial_metrics']['score'][0], reward=reward)
            after_physical = physical_key(env.state['current'])
            unchanged, revisited = memory.record(before_physical, after_physical, keys[index])
            step.update(vocabulary_index=index, observation_sha256=obs_hash, allowed_sha256=digest(allowed),
                allowed_count=sum(allowed), memory_exhausted=exhausted, old_log_probability=sampled_logp,
                sampling_entropy=sampled_entropy, probabilities_sha256=sampled_hash,
                current_before_sha256=before_witness, current_sha256=digest(env.state['current']),
                best_sha256=digest(env.state['best']), physical_before=before_physical, physical_after=after_physical,
                physical_unchanged=unchanged, physical_revisited=revisited, action_key=keys[index])
            if training:
                critic_obs_hash = observation_hash(critic_obs)
                experience.append(dict(actor_observation=obs, critic_observation=critic_obs,
                    observation_sha256=obs_hash, critic_observation_sha256=critic_obs_hash,
                    allowed=list(allowed), kinds=[a['kind'] for a in description['actions']], index=index,
                    old_log_probability=sampled_logp, old_value=old_value, reward=reward,
                    probabilities_sha256=sampled_hash))
                step.update(old_value=old_value, critic_observation_sha256=critic_obs_hash)
            history.append(history_record(action_token, before_score, step, env.state['initial_metrics']['score']))
            history = history[-16:]
        total = 0.0
        for step in reversed(env.trace):
            total += step['reward']
            step['reward_to_go'] = total
        summary = env.summary()
        summary.update(reward_schema=SCHEMA, reward_sum=total, gamma=1.0)
        payload = dict(schema=PPO_SCHEMA, steps=experience, critic_sha256=critic_hash) if training else None
        summary.update(schema_version=1, candidate=candidate, training=training, sample_seed=seed, policy_sha256=before_hash,
            observation_schema=obs['schema'] if env.selected else None, budget_complete=env.selected == config['horizon'] and
                time.perf_counter() - began < config['episode_seconds'] and not any(t['status'] == 'late' for t in env.trace))
        save_gzip(directory / 'episode.json.gz', summary)
        save_gzip(directory / 'best.json.gz', env.state['best'])
        if not summary['budget_complete']:
            raise TimeoutError(f'ppo-area.episode-budget: {directory}')
        if parameter_hash(model) != before_hash or parameter_hash(critic) != critic_hash:
            raise ValueError('ppo-area.behavior-mutated')
        row = dict(case=case['id'], instances=case['instances'], candidate=candidate, sample_seed=seed,
            selected=env.selected, directory=str(directory.resolve()), training=training,
            initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'],
            reward_sum=total, reward_schema=SCHEMA,
            kinds=dict(Counter(t['action']['kind'] for t in env.trace)), statuses=dict(Counter(t['status'] for t in env.trace)),
            improvements=dict(Counter(t['action']['kind'] for t in env.trace if t['status'] == 'improved')),
            route_expansions=sum(t['route_expansions'] for t in env.trace), seconds=time.perf_counter() - began,
            policy_sha256=before_hash, budget_complete=True)
        return row, payload
    except BaseException as error:
        if env is not None:
            save_experience(directory / 'experience-partial.pt.gz', dict(schema=PPO_SCHEMA, status='incomplete',
                policy_sha256=before_hash, critic_sha256=critic_hash, steps=experience))
            save_gzip(directory / 'current-partial.json.gz', env.state['current'])
            save_gzip(directory / 'episode-partial.json.gz', env.summary())
            save_gzip(directory / 'best.json.gz', env.state['best'])
        write(directory / 'partial.json', dict(status='incomplete', code='ppo-area.rollout-failed', severity='error',
            stage='rollout', entity=case['id'], message=str(error), optimizer_updates=0))
        raise
