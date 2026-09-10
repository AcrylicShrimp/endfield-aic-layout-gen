"""Exact-shape rasters and independent graphs for batched world encoders."""
from collections import defaultdict

import numpy as np
import torch
from torch.nn import functional as F


def prepare_worlds(worlds, device):
    def tensor(value, dtype=torch.float32):
        return torch.as_tensor(np.asarray(value), dtype=dtype, device=device)
    buckets = defaultdict(list)
    node_sizes, nodes, edges, values, messages = [], [], [], [], []
    offset = 0
    for i, world in enumerate(worlds):
        buckets[tuple(world['raster'].shape)].append(i)
        node_sizes.append(len(world['nodes']))
        nodes.append(world['nodes'])
        edges.append(world['edges'] + offset)
        values.append(world['edge_values'])
        messages.extend([world['messages']] * len(world['nodes']))
        offset += len(world['nodes'])
    graph_edges = tensor(np.concatenate(edges), torch.long)
    degree = torch.zeros(offset, 1, device=device).index_add_(
        0, graph_edges[:, 1], torch.ones(len(graph_edges), 1, device=device)).clamp_min(1)
    return dict(count=len(worlds), node_sizes=node_sizes, node_offsets=np.cumsum([0] + node_sizes[:-1]).tolist(),
                nodes=tensor(np.concatenate(nodes)), edges=graph_edges, edge_values=tensor(np.concatenate(values)),
                degree=degree, any_messages=any(messages), all_messages=all(messages),
                message_nodes=tensor(messages, torch.bool).unsqueeze(1),
                node_xy=[tensor(w['node_xy'], torch.long) for w in worlds],
                buckets=[dict(indices=ids, raster=tensor(np.stack([worlds[i]['raster'] for i in ids])))
                         for ids in buckets.values()])


def encode_worlds(encoder, batch):
    samples, rasters, raster_means = ([None] * batch['count'] for _ in range(3))
    for bucket in batch['buckets']:
        raster = encoder.encode_raster(bucket['raster'])
        for j, i in enumerate(bucket['indices']):
            xy = batch['node_xy'][i]
            rasters[i] = raster[j]
            samples[i] = raster[j, :, xy[:, 1], xy[:, 0]].T
            raster_means[i] = raster[j].mean((1, 2))
    h = F.silu(encoder.node(torch.cat((batch['nodes'], torch.cat(samples)), 1)).float())
    if batch['any_messages']:
        edge_h, edges = encoder.edge(batch['edge_values']).float(), batch['edges']
        for message, update, norm in zip(encoder.messages, encoder.updates, encoder.norms):
            incoming = F.silu(message(h[edges[:, 0]]).float() + edge_h)
            aggregate = torch.zeros_like(h).index_add_(0, edges[:, 1], incoming) / batch['degree']
            changed = norm(h + F.silu(update(torch.cat((h, aggregate), 1)).float()))
            h = changed if batch['all_messages'] else torch.where(batch['message_nodes'], changed, h)
    pooled = torch.stack([torch.cat((nodes.mean(0), raster_mean))
                          for nodes, raster_mean in zip(h.split(batch['node_sizes']), raster_means)])
    return h, rasters, pooled
