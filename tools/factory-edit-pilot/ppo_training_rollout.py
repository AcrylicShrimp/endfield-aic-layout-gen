"""Native-valid PPO collection with shared observations and bounded identity reuse."""
from collections import Counter
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryEpisode, digest
from full_factory_input_contracts import CONTRACTS, observation_hash, selected_action_token, history_record
from ppo_training_observation import observation_pair, WitnessIdentities
from full_factory_on_policy import parameter_hash, probability_hash
from direct_area_reward import SCHEMA
from ppo_area_model import SCHEMA as PPO_SCHEMA
from ppo_area_update import save_experience
from full_factory_screen import save_gzip, write
from full_factory_selection import SelectionMemory, distribution


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
        memory = SelectionMemory(env.state['current'])
        identities = WitnessIdentities()
        history = []
        history_length = CONTRACTS[candidate]['history']
        rng = torch.Generator().manual_seed(seed)
        while not env.done:
            guard()
            ordinal = env.selected
            description = env.describe()
            keys, allowed, exhausted = identities.mask(env.state, description, memory, config['memory'], candidate)
            obs, critic_obs = observation_pair(env.state, description, 1 - ordinal / config['horizon'], history, candidate, runtime, training)
            obs_hash = observation_hash(obs)
            before_best_area = env.state['best_metrics']['score'][0]
            before_physical, before_witness = identities.get(env.state['current'])
            with torch.no_grad():
                dist = distribution(model(obs) / config['temperature'], description['actions'], allowed, config['balanced'])
                index = int(torch.multinomial(dist.probs.detach(), 1, generator=rng))
                logp, entropy = dist.log_prob(torch.tensor(index)), dist.entropy()
                sampled_logp, sampled_entropy = float(logp.detach()), float(entropy.detach())
                sampled_hash = probability_hash(dist.probs)
                old_value = float(critic(critic_obs, allowed)) if training else None
            guard()
            if env.done:
                break
            token = selected_action_token(obs, index) if history_length else None
            before_score = env.state['current_metrics']['score']
            step = env.step(description['actions'][index])
            after_best_area = env.state['best_metrics']['score'][0]
            reward = (before_best_area - after_best_area) / env.state['initial_metrics']['score'][0]
            step.update(best_area_before=before_best_area, reward_numerator=before_best_area - after_best_area,
                        reward_denominator=env.state['initial_metrics']['score'][0], reward=reward)
            after_physical, after_witness = identities.get(env.state['current'])
            unchanged, revisited = memory.record(before_physical, after_physical, keys[index])
            step.update(vocabulary_index=index, observation_sha256=obs_hash, allowed_sha256=digest(allowed),
                allowed_count=sum(allowed), memory_exhausted=exhausted, old_log_probability=sampled_logp,
                sampling_entropy=sampled_entropy, probabilities_sha256=sampled_hash,
                current_before_sha256=before_witness, current_sha256=after_witness,
                best_sha256=identities.get(env.state['best'])[1], physical_before=before_physical, physical_after=after_physical,
                physical_unchanged=unchanged, physical_revisited=revisited, action_key=keys[index])
            if training:
                critic_obs_hash = observation_hash(critic_obs)
                experience.append(dict(actor_observation=obs, critic_observation=critic_obs,
                    observation_sha256=obs_hash, critic_observation_sha256=critic_obs_hash,
                    allowed=list(allowed), kinds=[a['kind'] for a in description['actions']], index=index,
                    old_log_probability=sampled_logp, old_value=old_value, reward=reward,
                    probabilities_sha256=sampled_hash))
                step.update(old_value=old_value, critic_observation_sha256=critic_obs_hash)
            if history_length:
                history.append(history_record(token, before_score, step, env.state['initial_metrics']['score']))
                history = history[-history_length:]
        total = 0.0
        for step in reversed(env.trace):
            total += step['reward']
            step['reward_to_go'] = total
        summary = env.summary()
        summary.update(reward_schema=SCHEMA, reward_sum=total, gamma=1.0)
        payload = dict(schema=PPO_SCHEMA, input_contract=candidate, steps=experience, critic_sha256=critic_hash) if training else None
        summary.update(schema_version=1, candidate=candidate, training=training, sample_seed=seed, policy_sha256=before_hash,
            observation_schema=obs['schema'] if env.selected else None, budget_complete=env.selected == config['horizon'] and
                time.perf_counter() - began < config['episode_seconds'] and not any(t['status'] == 'late' for t in env.trace))
        save_gzip(directory / 'episode.json.gz', summary)
        save_gzip(directory / 'best.json.gz', env.state['best'])
        if not summary['budget_complete']:
            raise TimeoutError(f'ppo-area.episode-budget: {directory}')
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
