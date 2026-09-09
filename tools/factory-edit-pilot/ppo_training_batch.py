"""Prepared PPO actors with optional worlds/history and a full-world critic."""
from collections import Counter
import math

import numpy as np
import torch

from full_factory_input_contracts import HISTORY_WIDTH
from ppo_training_world import prepare_worlds, encode_worlds

EXECUTION_SCHEMA = 'ppo-batched-input-contracts-v2'


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-training.contract: {message}')


def prepare_actor(observations, counts, width, device):
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype, device=device)
    worlds, current_indices, best_indices, references = [], [], [], []
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
        references.append(dict(current=current, actions=tensor(obs['action_xy'], torch.long),
                               members=tensor(obs['member_xy'], torch.long)))
    world_batch = prepare_worlds(worlds, device) if worlds else None
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
                best_indices=tensor(best_indices, torch.long), references=references,
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
        allowed[i, :count] = tensor(step['allowed'], torch.bool)
        frequencies = Counter(kind for kind, ok in zip(step['kinds'], step['allowed']) if ok)
        adjustments[i, :count] = tensor([-math.log(frequencies[kind]) if ok and config['balanced'] else 0
                                        for kind, ok in zip(step['kinds'], step['allowed'])])
        available = tensor(obs['actions'])[allowed[i, :count]]
        summaries.append(torch.cat((available.mean(0), available.max(0).values, tensor(obs['context']),
                                    allowed[i, :count].float().mean().reshape(1))))
        current_indices.append(len(worlds))
        worlds.append(obs['current'])
        if obs['best_is_current']:
            best_indices.append(current_indices[-1])
        else:
            best_indices.append(len(worlds))
            worlds.append(obs['best'])
    return dict(size=len(steps), counts=counts, width=width, actor=prepare_actor(actors, counts, width, device),
                allowed=allowed, adjustments=adjustments, chosen=tensor([s['index'] for s in steps], torch.long),
                old_logp=tensor([s['old_log_probability'] for s in steps]), old_values=tensor([s['old_value'] for s in steps]),
                summaries=torch.stack(summaries), critic_worlds=prepare_worlds(worlds, device),
                current_indices=tensor(current_indices, torch.long), best_indices=tensor(best_indices, torch.long))


def actor_logits(actor, batch):
    obs, width = batch['actor'], batch['width']
    if obs['worlds'] is None:
        operands, local, member_context = obs['zero_operands'], obs['zero_local'], obs['zero_members']
        current, best = obs['zero_global'], obs['zero_global']
    else:
        h, rasters, global_features = encode_worlds(actor.world, obs['worlds'])
        padded_nodes = torch.cat((h.new_zeros(1, width), h))
        operands = padded_nodes[obs['operands']].flatten(1)
        action_samples, member_samples = [], []
        for reference in obs['references']:
            index, xy, mxy = reference['current'], reference['actions'], reference['members']
            action_samples.append(rasters[index][:, xy[:, 1], xy[:, 0]].T if index >= 0 else h.new_zeros(len(xy), width))
            member_samples.append(rasters[index][:, mxy[:, 1], mxy[:, 0]].T if index >= 0 else h.new_zeros(len(mxy), width))
        local = torch.cat(action_samples)
        member_context = torch.cat((padded_nodes[obs['member_nodes']], torch.cat(member_samples)), 1)
        padded_global = torch.cat((global_features.new_zeros(1, 2 * width), global_features))
        current, best = padded_global[obs['current_indices']], padded_global[obs['best_indices']]
    members = actor.member(torch.cat((member_context, obs['member_pose']), 1)).float()
    pooled = torch.zeros_like(obs['zero_operands']).index_add_(0, obs['member_actions'], members) / obs['member_counts']
    history = obs['zero_history']
    if obs['has_history']:
        # A padded future cannot affect a unidirectional GRU's true endpoint.
        # Keep recurrence and its input normalization in FP32 during AMP probes.
        with torch.autocast(next(actor.parameters()).device.type, enabled=False):
            output, _ = actor.history(actor.history_input(obs['history']))
            history = torch.where(obs['history_present'], output[obs['history_rows'], obs['history_end']], history)
    global_h = torch.cat((current, best, history, obs['contexts']), 1)
    expanded = torch.cat([global_h[i].expand(count, -1) for i, count in enumerate(batch['counts'])])
    return actor.score(torch.cat((obs['actions'], operands, local, pooled, expanded), 1)).float().squeeze(1)


def distribution(actor, batch, config):
    logits = actor_logits(actor, batch).split(batch['counts'])
    padded = torch.nn.utils.rnn.pad_sequence(logits, batch_first=True, padding_value=-torch.inf)
    adjusted = (padded / config['temperature'] + batch['adjustments']).masked_fill(~batch['allowed'], -torch.inf)
    result = torch.distributions.Categorical(logits=adjusted, validate_args=False)
    entropy = -(result.probs * result.logits.masked_fill(~batch['allowed'], 0)).sum(1)
    return result.log_prob(batch['chosen']), entropy, result.probs


def critic_values(critic, batch):
    pooled = encode_worlds(critic.world, batch['critic_worlds'])[2]
    features = torch.cat((pooled[batch['current_indices']], pooled[batch['best_indices']], batch['summaries']), 1)
    return critic.head(features).float().squeeze(1)
