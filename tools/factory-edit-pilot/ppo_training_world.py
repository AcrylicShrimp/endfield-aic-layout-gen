"""Masked padded rasters and independent graphs for batched world encoders."""

import numpy as np
import torch
from torch.nn import functional as F

from ppo_training_spatial import prepare_samples, gather_samples, restore_samples, padded_rasters, encode_padded_raster


def prepare_worlds(worlds, device, samples=None):
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype, device=device)
    node_sizes, nodes, edges, values, messages = [], [], [], [], []
    offset = 0
    for world in worlds:
        node_sizes.append(len(world['nodes']))
        nodes.append(world['nodes'])
        edges.append(world['edges'] + offset)
        values.append(world['edge_values'])
        messages.extend([world['messages']] * len(world['nodes']))
        offset += len(world['nodes'])
    graph_edges = tensor(np.concatenate(edges), torch.long)
    degree = torch.zeros(offset, 1, device=device).index_add_(
        0, graph_edges[:, 1], torch.ones(len(graph_edges), 1, device=device)).clamp_min(1)
    groups = [list(range(len(worlds)))]
    raster, mask = padded_rasters(worlds, device)
    plans, orders = prepare_samples(worlds, groups,
        dict(nodes=[(i, world['node_xy']) for i, world in enumerate(worlds)], **(samples or {})), device,
        spatial_shapes=[raster.shape[2:]])
    return dict(count=len(worlds), node_sizes=node_sizes, node_offsets=np.cumsum([0] + node_sizes[:-1]).tolist(),
                nodes=tensor(np.concatenate(nodes)), edges=graph_edges, edge_values=tensor(np.concatenate(values)),
                degree=degree, any_messages=any(messages), all_messages=all(messages),
                message_nodes=tensor(messages, torch.bool).unsqueeze(1),
                sample_orders=orders,
                buckets=[dict(indices=groups[0], samples=plans[0], raster=raster, mask=mask)])


def encode_worlds(encoder, batch):
    parts = {name: [] for name in batch['sample_orders']}
    raster_means = [None] * batch['count']
    for bucket in batch['buckets']:
        raster = encode_padded_raster(encoder, bucket['raster'], bucket['mask'])
        for name, value in gather_samples(raster, bucket['samples']).items():
            parts[name].append(value)
        means = raster.sum((2, 3)) / bucket['mask'].sum((2, 3))
        for j, i in enumerate(bucket['indices']):
            raster_means[i] = means[j]
    samples = restore_samples(parts, batch['sample_orders'], raster)
    h = F.silu(encoder.node(torch.cat((batch['nodes'], samples.pop('nodes')), 1)).float())
    if batch['any_messages']:
        edge_h, edges = encoder.edge(batch['edge_values']).float(), batch['edges']
        for message, update, norm in zip(encoder.messages, encoder.updates, encoder.norms):
            incoming = F.silu(message(h[edges[:, 0]]).float() + edge_h)
            aggregate = torch.zeros_like(h).index_add_(0, edges[:, 1], incoming) / batch['degree']
            changed = norm(h + F.silu(update(torch.cat((h, aggregate), 1)).float()))
            h = changed if batch['all_messages'] else torch.where(batch['message_nodes'], changed, h)
    pooled = torch.stack([torch.cat((nodes.mean(0), raster_mean))
                          for nodes, raster_mean in zip(h.split(batch['node_sizes']), raster_means)])
    return h, samples, pooled
