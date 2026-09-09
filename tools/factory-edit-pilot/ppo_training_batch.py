"""Prepared action-only actor and exact-shape, disjoint-graph critic batches."""
from collections import Counter, defaultdict
import math

import numpy as np
import torch
from torch.nn import functional as F

EXECUTION_SCHEMA = 'ppo-batched-execution-v1'


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-training.contract: {message}')


def prepare(steps, config, width, device):
    """Prepare fixed observation data once; no model execution or behavior rewrite."""
    require(bool(steps), 'nonempty microbatch')
    device = torch.device(device)
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype, device=device)
    actors = [step['actor_observation'] for step in steps]
    critics = [step['critic_observation'] for step in steps]
    counts = [len(obs['actions']) for obs in actors]
    offsets = np.cumsum([0] + counts[:-1])
    member_actions = np.concatenate([obs['member_actions'] + offset for obs, offset in zip(actors, offsets)])
    member_pose = np.concatenate([obs['member_pose'] for obs in actors])
    member_indices = tensor(member_actions, torch.long)
    member_counts = torch.zeros(sum(counts), 1, device=device).index_add_(
        0, member_indices, torch.ones(len(member_actions), 1, device=device)).clamp_min(1)
    actions = tensor(np.concatenate([obs['actions'] for obs in actors]))
    contexts = tensor(np.concatenate([np.repeat(obs['context'][None, :], count, axis=0)
                                      for obs, count in zip(actors, counts)]))
    left = torch.cat((actions, torch.zeros(sum(counts), 3 * width, device=device)), 1)
    right = torch.cat((torch.zeros(sum(counts), 5 * width, device=device), contexts), 1)
    member_input = torch.cat((torch.zeros(len(member_actions), 2 * width, device=device), tensor(member_pose)), 1)
    allowed = torch.zeros(len(steps), max(counts), dtype=torch.bool, device=device)
    adjustments = torch.zeros(len(steps), max(counts), device=device)
    summaries, worlds, current_indices, best_indices = [], [], [], []
    for i, (step, obs, count) in enumerate(zip(steps, critics, counts)):
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
    raster_buckets = defaultdict(list)
    node_sizes, node_values, edges, edge_values = [], [], [], []
    node_offset = 0
    for i, world in enumerate(worlds):
        raster_buckets[tuple(world['raster'].shape)].append(i)
        node_sizes.append(len(world['nodes']))
        node_values.append(world['nodes'])
        edges.append(world['edges'] + node_offset)
        edge_values.append(world['edge_values'])
        node_offset += len(world['nodes'])
    graph_edges = tensor(np.concatenate(edges), torch.long)
    degree = torch.zeros(node_offset, 1, device=device).index_add_(
        0, graph_edges[:, 1], torch.ones(len(graph_edges), 1, device=device)).clamp_min(1)
    buckets = [dict(indices=indices, raster=tensor(np.stack([worlds[i]['raster'] for i in indices])))
               for indices in raster_buckets.values()]
    return dict(size=len(steps), counts=counts, width=width, actor_left=left, actor_right=right,
                member_input=member_input, member_actions=member_indices, member_counts=member_counts,
                allowed=allowed, adjustments=adjustments, chosen=tensor([s['index'] for s in steps], torch.long),
                old_logp=tensor([s['old_log_probability'] for s in steps]),
                old_values=tensor([s['old_value'] for s in steps]), summaries=torch.stack(summaries),
                worlds=len(worlds), raster_buckets=buckets, node_sizes=node_sizes,
                node_xy=[tensor(w['node_xy'], torch.long) for w in worlds], nodes=tensor(np.concatenate(node_values)),
                edges=graph_edges, edge_values=tensor(np.concatenate(edge_values)), degree=degree,
                current_indices=tensor(current_indices, torch.long), best_indices=tensor(best_indices, torch.long))


def actor_logits(actor, batch):
    members = actor.member(batch['member_input']).float()
    pooled = torch.zeros(len(batch['actor_left']), 2 * batch['width'], device=members.device).index_add_(
        0, batch['member_actions'], members) / batch['member_counts']
    features = torch.cat((batch['actor_left'], pooled, batch['actor_right']), 1)
    return actor.score(features).float().squeeze(1)


def distribution(actor, batch, config):
    logits = actor_logits(actor, batch).split(batch['counts'])
    padded = torch.nn.utils.rnn.pad_sequence(logits, batch_first=True, padding_value=-torch.inf)
    adjusted = (padded / config['temperature'] + batch['adjustments']).masked_fill(~batch['allowed'], -torch.inf)
    result = torch.distributions.Categorical(logits=adjusted, validate_args=False)
    # Mask before multiplying, including when a GradScaler scales the entropy loss.
    entropy = -(result.probs * result.logits.masked_fill(~batch['allowed'], 0)).sum(1)
    return result.log_prob(batch['chosen']), entropy, result.probs


def critic_values(critic, batch):
    encoder = critic.world
    samples, raster_means = [None] * batch['worlds'], [None] * batch['worlds']
    for bucket in batch['raster_buckets']:
        # GroupNorm sees only real pixels and normalizes every sample separately.
        raster = encoder.raster[0](bucket['raster']).float()
        raster = encoder.raster[2](encoder.raster[1](raster))
        raster = encoder.raster[4](encoder.raster[3](raster).float())
        for j, i in enumerate(bucket['indices']):
            xy = batch['node_xy'][i]
            samples[i] = raster[j, :, xy[:, 1], xy[:, 0]].T
            raster_means[i] = raster[j].mean((1, 2))
    h = F.silu(encoder.node(torch.cat((batch['nodes'], torch.cat(samples)), 1)).float())
    edge_h = encoder.edge(batch['edge_values']).float()
    edges = batch['edges']
    for message, update, norm in zip(encoder.messages, encoder.updates, encoder.norms):
        incoming = F.silu(message(h[edges[:, 0]]).float() + edge_h)
        aggregate = torch.zeros_like(h).index_add_(0, edges[:, 1], incoming) / batch['degree']
        h = norm(h + F.silu(update(torch.cat((h, aggregate), 1)).float()))
    pooled = torch.stack([torch.cat((nodes.mean(0), raster_mean))
                          for nodes, raster_mean in zip(h.split(batch['node_sizes']), raster_means)])
    features = torch.cat((pooled[batch['current_indices']], pooled[batch['best_indices']], batch['summaries']), 1)
    return critic.head(features).float().squeeze(1)
