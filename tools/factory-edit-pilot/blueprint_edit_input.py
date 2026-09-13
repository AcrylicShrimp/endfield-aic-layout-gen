"""Blueprint/draft graph contract and experimental actor/critic.

No existing PPO checkpoint or finite-horizon observation is adapted here.
Categorical fingerprints are features only; native IDs remain the authority.
"""
import hashlib
import math

import torch
from torch import nn

SCHEMA = 'blueprint-edit-graph-v1'
NODE_STATIC = 14
NODE_STATE = 10
EDGE_STATIC = 10
EDGE_STATE = 4
ACTION_FEATURES = 12
GLOBAL_FEATURES = 7
KINDS = ('pose', 'repair', 'repair_all', 'commit', 'restore_best')


def fingerprint(text):
    return [b / 255. for b in hashlib.sha256(text.encode()).digest()[:4]]


def rate(value):
    return value['numerator'] / value['denominator']


def prepare(observation):
    if observation['schema'] != 'blueprint-edit-observation-v1':
        raise ValueError('blueprint.input-schema: expected blueprint-edit-observation-v1')
    bp = observation['blueprint']
    facilities = observation['facilities']
    facility_index = {f['instance']: i for i, f in enumerate(facilities)}
    offset = len(facilities)
    scale = max(1, *(f['x'] + f['width'] for f in facilities),
                *(f['y'] + f['height'] for f in facilities),
                *(n['position']['x'] + 1 for n in observation['nodes']),
                *(n['position']['y'] + 1 for n in observation['nodes']))
    collisions = {f['instance']: 0. for f in facilities}
    for conflict in observation['conflicts']:
        for name in conflict.get('facilities', [conflict.get('facility')]):
            collisions[name] += 1
    static, state = [], []
    for f, original in zip(facilities, bp['facilities'], strict=True):
        if f['instance'] != original['instance']:
            raise ValueError('blueprint.input-order: original facility identities changed')
        static.append([1., 0., 0., 0., original['width'] / scale, original['height'] / scale,
                       rate(bp['utilizations'][f['instance']]), 0., 0., 0., *fingerprint(f['recipe'])])
        angle = math.radians(f['rotation'])
        state.append([f['x'] / scale, f['y'] / scale, f['width'] / scale, f['height'] / scale,
                      math.sin(angle), math.cos(angle), collisions[f['instance']], 0., 0., 1.])
    for node, current in zip(bp['nodes'], observation['nodes'], strict=True):
        if node['id'] != current['id']:
            raise ValueError('blueprint.input-order: original anchor identities changed')
        terminal = node['terminal']
        external = terminal is not None and terminal['endpoint']['kind'] == 'external'
        n = bp['networks'][node['network']]
        static.append([0., float(terminal is not None), float(bool(node['components'])), float(external),
                       0., 0., 0., (1. if terminal['direction'] == 'output' else -1.) if terminal else 0.,
                       rate(terminal['rate']) if terminal else 0., float(n['transport'] == 'pipe'), *fingerprint(n['item'])])
        state.append([current['position']['x'] / scale, current['position']['y'] / scale,
                      0., 0., 0., 0., 0., 0., 0., 1.])
    links, edge_static, edge_state = [], [], []
    path_cells, path_edges = [], []
    for edge, current in zip(bp['edges'], observation['edges'], strict=True):
        if edge['id'] != current['id']:
            raise ValueError('blueprint.input-order: original edge identities changed')
        network = bp['networks'][edge['network']]
        links.append([offset + edge['source'], offset + edge['target']])
        edge_static.append([1., 0., float(network['transport'] == 'pipe'), rate(edge['rate']),
                            (edge['first'] + 1) / 4 if edge['first'] is not None else 0.,
                            (edge['last'] + 1) / 4 if edge['last'] is not None else 0., *fingerprint(network['item'])])
        source, target = observation['nodes'][edge['source']]['position'], observation['nodes'][edge['target']]['position']
        edge_state.append([float(current['status'] == 'pending'), (len(current['path']) - 1) / scale if current['path'] else 0.,
                           (target['x'] - source['x']) / scale, (target['y'] - source['y']) / scale])
        for point in current['path'] or []:
            path_cells.append([point['x'] / scale, point['y'] / scale])
            path_edges.append(edge['id'])
    # Port-to-facility links preserve overlapping facility identities in the graph.
    for node in bp['nodes']:
        terminal = node['terminal']
        if terminal and terminal['endpoint']['kind'] == 'facility':
            links.append([facility_index[terminal['endpoint']['instance']], offset + node['id']])
            edge_static.append([0., 1., 0., 0., 0., 0., *fingerprint(terminal['endpoint']['port'])])
            edge_state.append([0.] * EDGE_STATE)

    actions, features, references = [], [], []
    def add(action, source=-1, target=-1, extra=None):
        onehot = [float(action['kind'] == kind) for kind in KINDS]
        actions.append(action)
        features.append(onehot + (extra or [0.] * 7))
        references.append([source, target])
    for i, facility in enumerate(facilities):
        destinations = {(facility['x'] + dx, facility['y'] + dy) for dx, dy in
                        ((1, 0), (-1, 0), (0, 1), (0, -1), (2, 0), (-2, 0), (0, 2), (0, -2))}
        destinations.update((other['x'], other['y']) for other in facilities)
        destinations.discard((facility['x'], facility['y']))
        for x, y in sorted(destinations):
            if x < 0 or y < 0:
                continue
            add(dict(kind='pose', instance=facility['instance'], x=x, y=y, rotation=facility['rotation']), i, i,
                [(x - facility['x']) / scale, (y - facility['y']) / scale, 0., 1.,
                 facility['width'] / scale, facility['height'] / scale, 0.])
    for edge in bp['edges']:
        add(dict(kind='repair', edge=edge['id']), offset + edge['source'], offset + edge['target'],
            [0.] * 6 + [float(observation['edges'][edge['id']]['status'] == 'pending')])
    for kind in ('repair_all', 'commit', 'restore_best'):
        add(dict(kind=kind))
    def tensor(rows, columns, dtype=torch.float32):
        return torch.tensor(rows, dtype=dtype).reshape(-1, columns)
    initial_score = observation['initial_metrics']['score']
    best_score = observation['best_metrics']['score']
    global_features = [best / max(initial, 1) for best, initial in zip(best_score, initial_score)]
    global_features += [sum(e['status'] == 'pending' for e in observation['edges']) / max(len(bp['edges']), 1),
                        sum(c.get('area', 1) for c in observation['conflicts']) / max(initial_score[0], 1),
                        observation['elapsed_ms'] / observation['config']['elapsed_ms'],
                        observation['attempts'] / observation['config']['max_actions']]
    return dict(schema=SCHEMA, blueprint_sha256=observation['blueprint_sha256'], revision=observation['revision'],
                node_static=tensor(static, NODE_STATIC), node_state=tensor(state, NODE_STATE),
                edge_index=tensor(links, 2, torch.long).T.contiguous(), edge_static=tensor(edge_static, EDGE_STATIC),
                edge_state=tensor(edge_state, EDGE_STATE), action_features=tensor(features, ACTION_FEATURES),
                path_cells=tensor(path_cells, 2), path_edges=torch.tensor(path_edges, dtype=torch.long),
                global_features=torch.tensor(global_features, dtype=torch.float32),
                action_references=tensor(references, 2, torch.long), actions=actions)


class BlueprintActorCritic(nn.Module):
    """Shared blueprint encoder with separate policy and value heads."""
    def __init__(self, width=40, message_rounds=2):
        super().__init__()
        if type(width) is not int or width <= 0 or type(message_rounds) is not int or message_rounds <= 0:
            raise ValueError('blueprint.model-config: width and message_rounds must be positive integers')
        self.nodes = nn.Sequential(nn.Linear(NODE_STATIC + NODE_STATE, width), nn.Tanh())
        self.edges = nn.Sequential(nn.Linear(EDGE_STATIC + EDGE_STATE, width), nn.Tanh())
        self.paths = nn.Sequential(nn.Linear(2, width), nn.Tanh())
        self.messages = nn.ModuleList(nn.Linear(width * 3, width) for _ in range(message_rounds))
        self.updates = nn.ModuleList(nn.Linear(width * 2, width) for _ in range(message_rounds))
        self.context = nn.Sequential(nn.Linear(width + GLOBAL_FEATURES, width), nn.Tanh())
        self.actor = nn.Sequential(nn.Linear(width * 3 + ACTION_FEATURES, width), nn.Tanh(), nn.Linear(width, 1))
        self.critic = nn.Sequential(nn.Linear(width, width), nn.Tanh(), nn.Linear(width, 1))

    def forward(self, batch):
        if batch['schema'] != SCHEMA:
            raise ValueError(f'blueprint.model-schema: expected {SCHEMA}')
        from blueprint_ppo_batch import pack
        logits, values = self.forward_packed(pack([batch]))
        return logits, values[0]

    packed_schema = 'blueprint-edit-packed-v1'

    def encode_packed(self, batch):
        if batch['schema'] != self.packed_schema:
            raise ValueError('blueprint.model-schema: expected disjoint packed graphs')
        nodes = self.nodes(torch.cat((batch['node_static'], batch['node_state']), dim=-1))
        edges = self.edges(torch.cat((batch['edge_static'], batch['edge_state']), dim=-1))
        route_sum = torch.zeros_like(edges)
        route_sum.index_add_(0, batch['path_edges'], self.paths(batch['path_cells']))
        count = batch['path_counts'].unsqueeze(-1)
        edges = edges + route_sum / count
        source, target = batch['edge_index']
        for message, update in zip(self.messages, self.updates):
            incoming = torch.zeros_like(nodes)
            # Ordered arguments distinguish upstream from downstream incidence.
            incoming.index_add_(0, target, torch.tanh(message(torch.cat((nodes[source], nodes[target], edges), dim=-1))))
            incoming.index_add_(0, source, torch.tanh(message(torch.cat((nodes[target], nodes[source], -edges), dim=-1))))
            nodes = torch.tanh(update(torch.cat((nodes, incoming), dim=-1)))
        node_graph = batch['node_graph']
        pooled = nodes.new_zeros((batch['size'], nodes.shape[1]))
        pooled.index_add_(0, node_graph, nodes)
        pooled = pooled / batch['node_counts'].unsqueeze(-1)
        pooled = self.context(torch.cat((pooled, batch['global_features']), dim=-1))
        return nodes, pooled

    def forward_packed(self, batch):
        nodes, pooled = self.encode_packed(batch)
        references = batch['action_references']
        selected = nodes[references.clamp_min(0)]
        action_context = pooled[batch['action_graph']]
        selected = torch.where((references >= 0).unsqueeze(-1), selected, action_context.unsqueeze(1))
        logits = self.actor(torch.cat((selected.flatten(1), action_context, batch['action_features']), dim=-1)).squeeze(-1)
        return logits, self.critic(pooled).squeeze(-1)
