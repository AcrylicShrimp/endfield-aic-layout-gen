"""Collect fixed behavior chunks while preserving every live native draft."""
import json
import time

import torch

from blueprint_edit_input import prepare
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_environment import semantic
from blueprint_ppo_update import digest, require


def step_stream(stream, observation, prepared, index, old_logp, old_value, entropy, config):
    action = prepared['actions'][index]
    outcome = stream.runtime.step(stream.session, observation, action)
    stream.policy_actions += 1
    after = json.loads(stream.session.observation_json())
    terminal = stream.policy_actions == config['training']['episode_actions']
    require(after['done'] == terminal and outcome['charged'], 'native guard interrupted the registered training episode')
    require(not any(d['code'] in ('blueprint.elapsed', 'blueprint.budget-exhausted') for d in outcome['diagnostics']), 'late training action')
    stream.counts[f'kind/{action["kind"]}'] += 1
    stream.counts[f'status/{outcome["status"]}'] += 1
    stream.counts['area_reward'] += outcome['reward']
    stream.counts['actions_with_pending'] += any(e['status'] == 'pending' for e in after['edges'])
    stream.counts['actions_with_overlap'] += bool(after['conflicts'])
    if outcome['status'] == 'improved':
        for boundary in (8, 32, 64):
            stream.counts[f'improvements_after_{boundary}'] += stream.policy_actions > boundary
    witness = None
    if action['kind'] == 'commit' and outcome['status'] in ('validated', 'improved'):
        witness = stream.runtime.commit_witness(stream.session, after)
    sample = dict(stream=stream.index, episode=stream.episode, start=stream.metadata['start'],
        policy_action=stream.policy_actions, input=prepared, index=index, old_logp=float(old_logp),
        old_value=float(old_value), entropy=float(entropy), reward=outcome['reward'], terminal=terminal,
        observation=observation, outcome=outcome, after_semantic=semantic(after),
        after_pending=sum(e['status'] == 'pending' for e in after['edges']), after_conflicts=len(after['conflicts']),
        validated_witness=witness)
    return sample, after


def finish_stream(stream, observation, prepared, value, config):
    terminal = stream.policy_actions == config['training']['episode_actions']
    frontier = dict(stream=stream.index, terminal=terminal, bootstrap=0. if terminal else float(value),
        successor_input=None if terminal else prepared, successor_observation=observation,
        semantic=semantic(observation))
    completed = None
    if terminal:
        result = stream.runtime.finish(stream.session, observation)
        completed = dict(stream=stream.index, episode=stream.episode, **stream.metadata,
            policy_actions=stream.policy_actions, counts=dict(stream.counts), result=result)
        stream.session = None
        stream.episode += 1
        stream.expected = None
    else:
        stream.expected = semantic(observation)
    return frontier, completed


def collect(model, streams, sampler, config, guard, partial=None):
    began = time.perf_counter()
    observations = [s.observe() for s in streams]
    before = [s.snapshot() for s in streams]
    behavior = digest(model.state_dict())
    samples = []
    if partial is not None:
        partial.update(native_before=before, samples=samples, model_before=behavior)
    for _ in range(config['training']['chunk_actions']):
        guard()
        inputs = [prepare(o) for o in observations]
        batch = pack(inputs)
        with torch.no_grad():
            logits, values = model.forward_packed(batch)
            logp, entropy = distribution(logits, batch)
        offset = 0
        afters = []
        for stream, observation, prepared, count, value, ent in zip(streams, observations, inputs, batch['action_counts'], values, entropy, strict=True):
            require(not observation['done'], 'a training episode ended inside a rollout chunk')
            probabilities = logp[offset:offset + count].exp()
            index = int(torch.multinomial(probabilities, 1, generator=sampler))
            sample, after = step_stream(stream, observation, prepared, index,
                logp[offset + index], value, ent, config)
            samples.append(sample)
            afters.append(after)
            offset += count
        observations = afters
    require(digest(model.state_dict()) == behavior, 'collection changed frozen behavior parameters')
    inputs = [prepare(o) for o in observations]
    with torch.no_grad():
        _, values = model.forward_packed(pack(inputs))
    frontiers, completed = [], []
    for stream, observation, prepared, value in zip(streams, observations, inputs, values, strict=True):
        frontier, episode = finish_stream(stream, observation, prepared, value, config)
        frontiers.append(frontier)
        if episode is not None:
            completed.append(episode)
    return dict(schema='blueprint-ppo-chunk-v1', model_before=behavior, native_before=before,
                samples=samples, frontiers=frontiers, completed=completed, collection_seconds=time.perf_counter() - began)
