"""Disjoint graph batching and segment-normalized blueprint action distributions."""
from collections import Counter
import math

import torch

SCHEMA = 'blueprint-edit-packed-v1'


def pack(observations, *, schema='blueprint-edit-graph-v1', packed_schema=SCHEMA):
    if not observations:
        raise ValueError('blueprint.batch: empty batch')
    fields = {k: [] for k in ('node_static', 'node_state', 'edge_static', 'edge_state', 'path_cells',
                              'path_edges', 'action_features', 'action_references', 'global_features')}
    nodes, actions, edges, counts, kinds = [], [], [], [], []
    node_offset = edge_offset = 0
    for index, observation in enumerate(observations):
        if observation['schema'] != schema:
            raise ValueError('blueprint.batch-schema: expected new graph contract')
        for key in ('node_static', 'node_state', 'edge_static', 'edge_state', 'path_cells', 'action_features'):
            fields[key].append(observation[key])
        fields['global_features'].append(observation['global_features'].unsqueeze(0))
        fields['path_edges'].append(observation['path_edges'] + edge_offset)
        references = observation['action_references']
        fields['action_references'].append(torch.where(references >= 0, references + node_offset, references))
        edges.append(observation['edge_index'] + node_offset)
        nodes.append(torch.full((len(observation['node_static']),), index, dtype=torch.long))
        actions.append(torch.full((len(observation['actions']),), index, dtype=torch.long))
        counts.append(len(observation['actions']))
        kind_log_counts=observation.get('kind_log_counts')
        if kind_log_counts is None:
            counts_by_kind=Counter(a['kind'] for a in observation['actions'])
            logarithms={kind:math.log(count) for kind,count in counts_by_kind.items()}
            kind_log_counts=torch.tensor([logarithms[a['kind']] for a in observation['actions']],dtype=torch.float32)
        kinds.append(kind_log_counts)
        node_offset += len(observation['node_static'])
        edge_offset += len(observation['edge_static'])
    result = dict(schema=packed_schema, size=len(observations), action_counts=counts,
                **{k: torch.cat(v) for k, v in fields.items()}, edge_index=torch.cat(edges, dim=1),
                node_graph=torch.cat(nodes), action_graph=torch.cat(actions),
                kind_log_counts=torch.cat(kinds))
    # Compute integer segment lengths during CPU preparation, avoiding device
    # synchronization for output-size inference in CUDA bincount.
    result['path_counts'] = torch.bincount(result['path_edges'], minlength=edge_offset).clamp_min(1)
    result['node_counts'] = torch.bincount(result['node_graph'], minlength=result['size']).clamp_min(1)
    return result


def to_device(batch, device):
    return {key: value.to(device, non_blocking=True) if isinstance(value, torch.Tensor) else value
            for key, value in batch.items()}


def distribution(logits, batch):
    adjusted = logits - batch['kind_log_counts']
    graph = batch['action_graph']
    maximum = adjusted.new_full((batch['size'],), -torch.inf)
    maximum.scatter_reduce_(0, graph, adjusted, reduce='amax', include_self=True)
    shifted = adjusted - maximum[graph]
    denominator = torch.zeros_like(maximum).index_add_(0, graph, shifted.exp())
    logp = shifted - denominator.log()[graph]
    entropy = torch.zeros_like(maximum).index_add_(0, graph, -logp.exp() * logp)
    return logp, entropy


def selected_indices(batch, indices):
    offset = 0
    result = []
    for count, index in zip(batch['action_counts'], indices, strict=True):
        if not 0 <= index < count:
            raise ValueError('blueprint.batch-action: action index outside its recorded candidates')
        result.append(offset + index)
        offset += count
    return torch.tensor(result, dtype=torch.long)
