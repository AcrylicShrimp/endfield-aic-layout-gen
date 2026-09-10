"""Prepared PPO actors with optional worlds/history and a full-world critic."""
from collections import Counter
import math

import numpy as np
import torch

from full_factory_input_contracts import HISTORY_WIDTH
from ppo_training_world import prepare_worlds, encode_worlds

EXECUTION_SCHEMA = 'ppo-batched-input-contracts-v3'


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-training.contract: {message}')


def static_inputs(step, config):
    """These CPU features depend on recorded observations, never on weights."""
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype)
    allowed = tensor(step['allowed'], torch.bool)
    frequencies = Counter(kind for kind, ok in zip(step['kinds'], step['allowed']) if ok)
    adjustment = tensor([-math.log(frequencies[kind]) if ok and config['balanced'] else 0
                         for kind, ok in zip(step['kinds'], step['allowed'])])
    obs = step['critic_observation']
    available = tensor(obs['actions'])[allowed]
    summary = torch.cat((available.mean(0), available.max(0).values, tensor(obs['context']),
                         allowed.float().mean().reshape(1)))
    return allowed, adjustment, summary


def compile_steps(steps, config):
    """Create update-local feature caches without modifying or retaining payloads."""
    return [dict(step, _static_inputs=static_inputs(step, config), _static_balanced=config['balanced']) for step in steps]


def prepare_actor(observations, counts, width, device):
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype, device=device)
    worlds, current_indices, best_indices = [], [], []
    samples = dict(actions=[], members=[])
    for obs in observations:
        current = len(worlds) if obs['current'] is not None else -1
        if current >= 0:
            worlds.append(obs['current'])
        best = -1
        if obs['best'] is not None:
            best = current if obs['best_is_current'] else len(worlds)
            if not obs['best_is_current']:
                worlds.append(obs['best'])
        current_indices.append(current + 1)
        best_indices.append(best + 1)
        samples['actions'].append((current, obs['action_xy']))
        samples['members'].append((current, obs['member_xy']))
    world_batch = prepare_worlds(worlds, device, samples) if worlds else None
    operands, member_nodes = [], []
    for obs, current in zip(observations, current_indices):
        offset = world_batch['node_offsets'][current - 1] if current else 0
        operands.append(np.where(obs['operands'] > 0, obs['operands'] + offset, 0) if current else np.zeros_like(obs['operands']))
        member_nodes.append(obs['member_nodes'] + offset + 1 if current else np.zeros_like(obs['member_nodes']))
    offsets = np.cumsum([0] + counts[:-1])
    member_actions = np.concatenate([obs['member_actions'] + offset for obs, offset in zip(observations, offsets)])
    member_indices = tensor(member_actions, torch.long)
    member_counts = torch.zeros(sum(counts), 1, device=device).index_add_(
        0, member_indices, torch.ones(len(member_actions), 1, device=device)).clamp_min(1)
    lengths = [len(obs['history']) for obs in observations]
    history = np.zeros((len(observations), max(lengths), HISTORY_WIDTH), np.float32)
    for i, obs in enumerate(observations):
        history[i, :lengths[i]] = obs['history']
    return dict(worlds=world_batch, current_indices=tensor(current_indices, torch.long),
                best_indices=tensor(best_indices, torch.long),
                operands=tensor(np.concatenate(operands), torch.long), member_nodes=tensor(np.concatenate(member_nodes), torch.long),
                member_pose=tensor(np.concatenate([obs['member_pose'] for obs in observations])),
                member_actions=member_indices, member_counts=member_counts,
                actions=tensor(np.concatenate([obs['actions'] for obs in observations])),
                contexts=tensor(np.stack([obs['context'] for obs in observations])),
                history=tensor(history), has_history=bool(max(lengths)),
                history_end=tensor([max(0, n - 1) for n in lengths], torch.long),
                history_present=tensor([n > 0 for n in lengths], torch.bool).unsqueeze(1),
                history_rows=torch.arange(len(observations), device=device),
                zero_operands=torch.zeros(sum(counts), 2 * width, device=device),
                zero_local=torch.zeros(sum(counts), width, device=device),
                zero_members=torch.zeros(len(member_actions), 2 * width, device=device),
                zero_global=torch.zeros(len(observations), 2 * width, device=device),
                zero_history=torch.zeros(len(observations), width, device=device))


def prepare(steps, config, width, device):
    """Prepare trusted observations without model execution or replay validation."""
    require(bool(steps), 'nonempty microbatch')
    device = torch.device(device)
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype, device=device)
    actors = [step['actor_observation'] for step in steps]
    counts = [len(obs['actions']) for obs in actors]
    allowed = torch.zeros(len(steps), max(counts), dtype=torch.bool, device=device)
    adjustments = torch.zeros(len(steps), max(counts), device=device)
    summaries, worlds, current_indices, best_indices = [], [], [], []
    for i, (step, count) in enumerate(zip(steps, counts)):
        obs = step['critic_observation']
        if '_static_inputs' in step:
            require(step['_static_balanced'] == config['balanced'], 'compiled input belongs to a different balancing configuration')
            mask, adjustment, summary = step['_static_inputs']
        else:
            mask, adjustment, summary = static_inputs(step, config)
        allowed[i, :count] = mask.to(device)
        adjustments[i, :count] = adjustment.to(device)
        summaries.append(summary.to(device))
        current_indices.append(len(worlds))
        worlds.append(obs['current'])
        if obs['best_is_current']:
            best_indices.append(current_indices[-1])
        else:
            best_indices.append(len(worlds))
            worlds.append(obs['best'])
    treatment = config.get('actor_critic')
    if treatment is not None:
        for step in steps:
            expected = step['actor_observation']['history']
            if not treatment['critic_history']:
                expected = expected[:0]
            require(np.array_equal(step['critic_observation']['history'], expected), 'critic history must match treatment')
    shared = treatment is not None and treatment['shared_backbone']
    if shared:
        require(all(a['current'] is not None and a['best'] is None for a in actors),
                'shared combination requires the fixed current-history actor input')
        combined = [dict(a, best=s['critic_observation']['best'],
                         best_is_current=s['critic_observation']['best_is_current']) for a, s in zip(actors, steps)]
        actor_batch = prepare_actor(combined, counts, width, device)
        current_index = actor_batch['current_indices'] - 1
        best_index = actor_batch['best_indices'] - 1
        # Best worlds are encoded for the critic but absent from actor features.
        actor_batch['best_indices'] = torch.zeros_like(actor_batch['best_indices'])
        critic_worlds = None
    else:
        actor_batch = prepare_actor(actors, counts, width, device)
        current_index, best_index = tensor(current_indices, torch.long), tensor(best_indices, torch.long)
        critic_worlds = prepare_worlds(worlds, device)
    return dict(size=len(steps), counts=counts, width=width, actor=actor_batch,
                allowed=allowed, adjustments=adjustments, chosen=tensor([s['index'] for s in steps], torch.long),
                old_logp=tensor([s['old_log_probability'] for s in steps]), old_values=tensor([s['old_value'] for s in steps]),
                summaries=torch.stack(summaries), critic_worlds=critic_worlds, shared_backbone=shared,
                current_indices=current_index, best_indices=best_index)


def history_features(model, obs):
    history = obs['zero_history']
    if obs['has_history']:
        # Padded future tokens cannot affect a unidirectional GRU's endpoint.
        with torch.autocast(next(model.parameters()).device.type, enabled=False):
            output, _ = model.history(model.history_input(obs['history']))
            history = torch.where(obs['history_present'], output[obs['history_rows'], obs['history_end']], history)
    return history


def actor_logits(actor, batch, *, encoded=None, history=None):
    obs, width = batch['actor'], batch['width']
    if obs['worlds'] is None:
        operands, local, member_context = obs['zero_operands'], obs['zero_local'], obs['zero_members']
        current, best = obs['zero_global'], obs['zero_global']
    else:
        h, spatial, global_features = encode_worlds(actor.world, obs['worlds']) if encoded is None else encoded
        padded_nodes = torch.cat((h.new_zeros(1, width), h))
        operands = padded_nodes[obs['operands']].flatten(1)
        local = spatial['actions']
        member_context = torch.cat((padded_nodes[obs['member_nodes']], spatial['members']), 1)
        padded_global = torch.cat((global_features.new_zeros(1, 2 * width), global_features))
        current, best = padded_global[obs['current_indices']], padded_global[obs['best_indices']]
    members = actor.member(torch.cat((member_context, obs['member_pose']), 1)).float()
    pooled = torch.zeros_like(obs['zero_operands']).index_add_(0, obs['member_actions'], members) / obs['member_counts']
    history = history_features(actor, obs) if history is None else history
    global_h = torch.cat((current, best, history, obs['contexts']), 1)
    expanded = torch.cat([global_h[i].expand(count, -1) for i, count in enumerate(batch['counts'])])
    return actor.score(torch.cat((obs['actions'], operands, local, pooled, expanded), 1)).float().squeeze(1)


def distribution(actor, batch, config, *, encoded=None, history=None):
    logits = actor_logits(actor, batch, encoded=encoded, history=history).split(batch['counts'])
    padded = torch.nn.utils.rnn.pad_sequence(logits, batch_first=True, padding_value=-torch.inf)
    adjusted = (padded / config['temperature'] + batch['adjustments']).masked_fill(~batch['allowed'], -torch.inf)
    result = torch.distributions.Categorical(logits=adjusted, validate_args=False)
    entropy = -(result.probs * result.logits.masked_fill(~batch['allowed'], 0)).sum(1)
    return result.log_prob(batch['chosen']), entropy, result.probs


def critic_values(critic, batch, *, pooled=None, history=None):
    worlds = batch['actor']['worlds'] if batch.get('shared_backbone') else batch['critic_worlds']
    pooled = encode_worlds(critic.world, worlds)[2] if pooled is None else pooled
    features = torch.cat((pooled[batch['current_indices']], pooled[batch['best_indices']], batch['summaries']), 1)
    if hasattr(critic, 'treatment'):
        if critic.treatment['critic_history']:
            history = history_features(critic, batch['actor']) if history is None else history
        else:
            history = batch['actor']['zero_history']
        features = torch.cat((features, history), 1)
    return critic.head(features).float().squeeze(1)


def predictions(actor, critic, batch, config):
    if batch.get('shared_backbone'):
        require(actor.world is critic.world and actor.history is critic.history, 'shared model identity')
        encoded = encode_worlds(actor.world, batch['actor']['worlds'])
        history = history_features(actor, batch['actor'])
        policy = distribution(actor, batch, config, encoded=encoded, history=history)
        values = critic_values(critic, batch, pooled=encoded[2], history=history)
    else:
        policy = distribution(actor, batch, config)
        values = critic_values(critic, batch)
    return (*policy, values)
