"""PPO sampling over Rust-owned observations, selection memory and factory edits."""
from collections import Counter
import json
from pathlib import Path
import time

import torch

from full_factory_input_contracts import observation_hash
from full_factory_on_policy import parameter_hash, probability_hash
from ppo_reward import SCHEMA, DEFAULT, FAILURES, apply, totals
from ppo_area_model import SCHEMA as PPO_SCHEMA
from ppo_area_update import save_experience
from full_factory_screen import save_gzip, write
from full_factory_selection import distribution_kinds
from ppo_episode_metrics import episode_metrics


def rollout(runtime, protocol, case, model, critic, candidate, seed, directory, training, guard):
    began = time.perf_counter()
    directory = Path(directory)
    directory.mkdir()
    config = protocol['config']
    reward_config = config.get('reward', DEFAULT)
    before_hash = parameter_hash(model)
    env = None
    experience = []
    trace = []
    critic_hash = parameter_hash(critic)
    try:
        env = runtime.open_episode(case, config['horizon'], config['episode_seconds'], config['route_work'],
                                   candidate, config['memory'])
        rng = torch.Generator().manual_seed(seed)
        while not env.done:
            guard()
            view = env.observe(training)
            obs, critic_obs, allowed = view['actor'], view['critic'], view['allowed']
            obs_hash = observation_hash(obs)
            with torch.no_grad():
                dist = distribution_kinds(model(obs) / config['temperature'], view['kind_ids'], allowed, config['balanced'])
                index = int(torch.multinomial(dist.probs.detach(), 1, generator=rng))
                sampled_logp = float(dist.log_prob(torch.tensor(index)))
                sampled_entropy = float(dist.entropy())
                sampled_hash = probability_hash(dist.probs)
                old_value = float(critic(critic_obs, allowed)) if training else None
            guard()
            if env.done:
                break
            step = env.step(view['handle'], index)
            apply(step, reward_config)
            step.update(observation_sha256=obs_hash, old_log_probability=sampled_logp,
                        sampling_entropy=sampled_entropy, probabilities_sha256=sampled_hash)
            if training:
                critic_obs_hash = observation_hash(critic_obs)
                experience.append(dict(actor_observation=obs, critic_observation=critic_obs,
                    observation_sha256=obs_hash, critic_observation_sha256=critic_obs_hash,
                    allowed=allowed.tolist(), kinds=[runtime.kinds[k] for k in view['kind_ids']], index=index,
                    old_log_probability=sampled_logp, old_value=old_value, reward=step['reward'],
                    status=step['status'], area_reward=step['area_reward'], failure_cost=step['failure_cost'],
                    probabilities_sha256=sampled_hash))
                step.update(old_value=old_value, critic_observation_sha256=critic_obs_hash)
            trace.append(step)
        total = 0.0
        for step in reversed(trace):
            total += step['reward']
            step['reward_to_go'] = total
        summary = dict(env.summary(), case=case['id'], trace=trace)
        accounting = totals(trace, reward_config)
        progress = episode_metrics(trace)
        summary.update(**accounting, **progress, gamma=1.0)
        payload = dict(schema=PPO_SCHEMA, reward_schema=SCHEMA, reward_config=dict(reward_config),
                       input_contract=candidate, steps=experience, critic_sha256=critic_hash) if training else None
        summary.update(schema_version=1, candidate=candidate, training=training, sample_seed=seed, policy_sha256=before_hash,
            observation_schema=obs['schema'] if env.selected else None, budget_complete=env.selected == config['horizon'] and
                time.perf_counter() - began < config['episode_seconds'] and not any(t['status'] == 'late' for t in trace))
        save_gzip(directory / 'episode.json.gz', summary)
        save_gzip(directory / 'best.json.gz', json.loads(env.export_layout('best')))
        if not summary['budget_complete']:
            raise TimeoutError(f'ppo-area.episode-budget: {directory}')
        row = dict(case=case['id'], instances=case['instances'], candidate=candidate, sample_seed=seed,
            selected=env.selected, directory=str(directory.resolve()), training=training,
            initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'],
            best_sha256=summary['best_sha256'],
            **accounting, **progress,
            kinds=dict(Counter(t['action']['kind'] for t in trace)), statuses=dict(Counter(t['status'] for t in trace)),
            improvements=dict(Counter(t['action']['kind'] for t in trace if t['status'] == 'improved')),
            route_expansions=sum(t['route_expansions'] for t in trace),
            executor_seconds=sum(t['executor_seconds'] for t in trace),
            routing_failure_codes=dict(Counter(d['code'] for t in trace if t['status'] in FAILURES
                                               for d in {d['code']: d for d in t['diagnostics']}.values())),
            routing_budget_failed_actions=sum(any(d['code'] in ('local-work-budget', 'region-work-exhausted')
                                                  for d in t['diagnostics']) for t in trace),
            routing_first_attempt_budget_failures=sum(t.get('region', {}).get('termination') == 'work-exhausted'
                and len(t['region']['attempts']) == 1 for t in trace),
            route_preflight_expansions=sum(r.get('preflight_expanded', 0) for t in trace
                for a in t.get('region', {}).get('attempts', []) for r in a['routes']),
            seconds=time.perf_counter() - began,
            policy_sha256=before_hash, budget_complete=True)
        return row, payload
    except BaseException as error:
        if env is not None:
            save_experience(directory / 'experience-partial.pt.gz', dict(schema=PPO_SCHEMA, reward_schema=SCHEMA, reward_config=dict(reward_config), status='incomplete',
                policy_sha256=before_hash, critic_sha256=critic_hash, steps=experience))
            save_gzip(directory / 'current-partial.json.gz', json.loads(env.export_layout('current')))
            partial = dict(env.summary(), case=case['id'])
            partial['trace'][:len(trace)] = trace
            save_gzip(directory / 'episode-partial.json.gz', partial)
            save_gzip(directory / 'best.json.gz', json.loads(env.export_layout('best')))
        write(directory / 'partial.json', dict(status='incomplete', code='ppo-area.rollout-failed', severity='error',
            stage='rollout', entity=case['id'], message=str(error), optimizer_updates=0))
        raise
