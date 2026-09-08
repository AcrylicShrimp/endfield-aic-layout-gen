"""Schema-two full-factory spatial / sparse relational policy; no legacy weights."""
import math

import numpy as np
import torch
from torch import nn

SCHEMA = 'full-factory-spatial-relational-v2'
KINDS = ('relocate', 'rotate', 'port', 'reroute', 'component', 'release_bridge', 'insert', 'contract', 'restore_best')
NODE_WIDTH, EDGE_WIDTH, ACTION_WIDTH, RASTER_WIDTH, CONTEXT_WIDTH = 32, 16, 32, 28, 8


def rate_features(rate):
    n, d = rate['numerator'], rate['denominator']
    return [n/d, math.log1p(n), math.log1p(d)]


def observe(w, description, state, remaining):
    """IDs address relationships only. Rates retain value and numerator/denominator signals."""
    if description.get('schema_version') != 2:
        raise ValueError('factory.description-schema: expected schema 2')
    box = state['current_metrics']
    # Accepted states are canonically translated, including external terminals.
    width, height = box['width'], box['height']
    sx, sy = max(1, width-1), max(1, height-1)
    raster = np.zeros((RASTER_WIDTH, height, width), np.float32)
    raster[25] = np.arange(width)[None, :]/sx
    raster[26] = np.arange(height)[:, None]/sy
    raster[27] = 1
    nodes, xy, edges, edge_values = [], [], [], []
    refs = {'facility': {}, 'network': {}, 'cell': {}, 'terminal': {}, 'component': {}, 'port': {}}

    def node(kind, p, transport=None):
        f = np.zeros(NODE_WIDTH, np.float32)
        f[kind] = 1
        f[7:9] = [p['x']/sx, p['y']/sy]
        if transport:
            f[11 + int(transport == 'pipe')] = 1
        i = len(nodes)
        nodes.append(f)
        xy.append([min(width-1, max(0, p['x'])), min(height-1, max(0, p['y']))])
        return i

    def edge(a, b, kind, rate=None, vector=None):
        f = np.zeros(EDGE_WIDTH, np.float32)
        f[kind] = 1
        if rate is not None:
            f[11:14] = rate_features(rate)
        if vector is not None:
            f[14:16] = vector
        edges.append([a, b])
        edge_values.append(f)

    def pair(a, b, kind):
        edge(a, b, kind)
        edge(b, a, kind+1)

    for p in w['placements']:
        i = node(0, p)
        refs['facility'][p['instance']] = i
        nodes[i][9:11] = [p['width']/width, p['height']/height]
        angle = math.radians(p['rotation'])
        nodes[i][15:17] = [math.sin(angle), math.cos(angle)]
        nodes[i][17:20] = rate_features(w['utilizations'][p['instance']])
        raster[0, p['y']:p['y']+p['height'], p['x']:p['x']+p['width']] = 1
    for ni, n in enumerate(w['transport_networks']):
        layer = int(n['transport'] == 'pipe')
        i = node(1, dict(x=0, y=0), n['transport'])
        refs['network'][ni] = i
        nodes[i][20:22] = [math.log1p(len(n['cells'])), math.log1p(len(n['terminals']))]
        for p in n['cells']:
            c = node(2, p, n['transport'])
            refs['cell'][ni, p['x'], p['y']] = c
            pair(c, i, 2)
            raster[1+layer, p['y'], p['x']] = 1
        for s in n['segments']:
            a, b = s['from'], s['to']
            ca, cb = (refs['cell'][ni, p['x'], p['y']] for p in (a, b))
            dx, dy = b['x']-a['x'], b['y']-a['y']
            direction = ((1, 0), (-1, 0), (0, 1), (0, -1)).index((dx, dy))
            edge(ca, cb, 0, s['rate'], [dx, dy])
            edge(cb, ca, 1, s['rate'], [-dx, -dy])
            nodes[ca][20] += 1
            nodes[cb][21] += 1
            nodes[ca][22] += s['rate']['numerator']/s['rate']['denominator']
            nodes[cb][23] += s['rate']['numerator']/s['rate']['denominator']
            raster[3+layer*8+direction, a['y'], a['x']] = 1
            raster[7+layer*8+direction, b['y'], b['x']] = 1
        for t in n['terminals']:
            ti = node(3 if t['endpoint']['kind'] == 'facility' else 6, t['position'], n['transport'])
            refs['terminal'][t['id']] = ti
            nodes[ti][13 + int(t['direction'] == 'output')] = 1
            nodes[ti][17:20] = rate_features(t['rate'])
            pair(ti, refs['cell'][ni, t['position']['x'], t['position']['y']], 4)
            if t['endpoint']['kind'] == 'facility':
                pair(ti, refs['facility'][t['endpoint']['instance']], 6)
    for c in w['logistics_components']:
        i = node(4, c['position'], c['transport'])
        refs['component'][c['id']] = i
        kind = ('splitter', 'converger', 'bridge').index(c['kind'])
        nodes[i][24+kind] = 1
        angle = math.radians(c['rotation'])
        nodes[i][15:17] = [math.sin(angle), math.cos(angle)]
        raster[19+3*int(c['transport'] == 'pipe')+kind, c['position']['y'], c['position']['x']] = 1
        for ni, n in enumerate(w['transport_networks']):
            if c['id'] in n['component_ids']:
                pair(i, refs['cell'][ni, c['position']['x'], c['position']['y']], 8)
    for instance, ports in description['ports'].items():
        for p in ports:
            i = node(5, p['connection'], p['transport'])
            refs['port'][instance, p['id']] = i
            nodes[i][13 + int(p['direction'] == 'output')] = 1
            edge_index = ('east', 'west', 'south', 'north').index(p['edge'])
            nodes[i][27+edge_index] = 1
            pair(i, refs['facility'][instance], 6)
    for n in w['transport_networks']:
        for t in n['terminals']:
            ep = t['endpoint']
            if ep['kind'] == 'facility':
                port = refs['port'][ep['instance'], ep['port']]
                nodes[port][31] = 1
                pair(refs['terminal'][t['id']], port, 8)
    # Commodity equality is structural information; raw item strings never enter tensors.
    for ai, a in enumerate(w['transport_networks']):
        for bi, b in enumerate(w['transport_networks'][:ai]):
            if (a['item'], a['transport']) == (b['item'], b['transport']):
                edge(refs['network'][ai], refs['network'][bi], 10)
                edge(refs['network'][bi], refs['network'][ai], 10)
    actions, operands, locations = [], [], []
    member_nodes, member_actions, member_xy = [], [], []
    for ai, a in enumerate(description['actions']):
        f = np.zeros(ACTION_WIDTH, np.float32)
        f[KINDS.index(a['kind'])] = 1
        f[9:11] = [a['x']/sx, a['y']/sy]
        f[11:13] = [a.get('dx', 0)/sx, a.get('dy', 0)/sy]
        angle = math.radians(a.get('rotation', 0))
        f[13:15] = [math.sin(angle), math.cos(angle)]
        if 'transport' in a:
            f[15+int(a['transport'] == 'pipe')] = 1
        if 'axis' in a:
            f[17+int(a['axis'] == 'y')] = 1
            f[19] = a['position']/(sx if a['axis'] == 'x' else sy)
        op = refs['facility'].get(a.get('instance'), -1)
        secondary = -1
        if 'component' in a:
            op = refs['component'][a['component']]
        if a['kind'] == 'port':
            op, secondary = refs['terminal'][a['terminal']], refs['port'][a['instance'], a['port']]
        if a['kind'] == 'reroute':
            c = description['chains'][a['chain']]
            op, secondary = (refs['cell'][c['network'], p['x'], p['y']] for p in (c['cells'][0], c['cells'][-1]))
            f[20:23] = rate_features(c['rate'])
            f[23] = math.log1p(len(c['cells']))
        if a['kind'] == 'relocate':
            members = a['members']
            if not members or len(members) > 16 or len(set(members)) != len(members) or any(type(i) is not int or i < 0 or i >= len(w['placements']) for i in members):
                raise ValueError('factory.relocation-members')
            f[24:30] = [math.log1p(len(members)), a['width']/width, a['height']/height,
                        a['body_area']/(width*height), a['source_x']/sx, a['source_y']/sy]
            for index in members:
                p = w['placements'][index]
                member_nodes.append(refs['facility'][p['instance']])
                member_actions.append(ai)
                member_xy.append([min(width-1, max(0, p['x']+(p['width']-1)//2+a['dx'])),
                                  min(height-1, max(0, p['y']+(p['height']-1)//2+a['dy']))])
        actions.append(f)
        operands.append([op+1, secondary+1])  # 0 is the explicit absent operand.
        locations.append([min(width-1, max(0, a['x'])), min(height-1, max(0, a['y']))])
    initial = state['initial_metrics']['score']
    current, best = state['current_metrics']['score'], state['best_metrics']['score']
    context = [remaining, math.log1p(width), math.log1p(height), math.log1p(len(w['placements']))]
    context += [current[i]/max(1, initial[i]) for i in range(2)] + [best[i]/max(1, initial[i]) for i in range(2)]
    return dict(schema=SCHEMA, raster=raster, nodes=np.asarray(nodes), node_xy=np.asarray(xy),
                edges=np.asarray(edges, dtype=np.int64).reshape(-1, 2), edge_values=np.asarray(edge_values).reshape(-1, EDGE_WIDTH),
                actions=np.asarray(actions), operands=np.asarray(operands), action_xy=np.asarray(locations),
                member_nodes=np.asarray(member_nodes, dtype=np.int64), member_actions=np.asarray(member_actions, dtype=np.int64),
                member_xy=np.asarray(member_xy, dtype=np.int64).reshape(-1, 2),
                context=np.asarray(context, dtype=np.float32))


class FullFactoryPolicy(nn.Module):
    def __init__(self, depth, width=32):
        super().__init__()
        self.depth, self.width = depth, width
        self.raster = nn.Sequential(nn.Conv2d(RASTER_WIDTH, width, 3, padding=1), nn.GroupNorm(4, width), nn.SiLU(),
                                    nn.Conv2d(width, width, 3, padding=1), nn.SiLU())
        self.node = nn.Linear(NODE_WIDTH+width, width)
        self.edge = nn.Linear(EDGE_WIDTH, width)
        self.messages = nn.ModuleList(nn.Linear(width, width) for _ in range(depth))
        self.updates = nn.ModuleList(nn.Linear(2*width, width) for _ in range(depth))
        self.norms = nn.ModuleList(nn.LayerNorm(width) for _ in range(depth))
        self.score = nn.Sequential(nn.Linear(ACTION_WIDTH+7*width+CONTEXT_WIDTH, width), nn.SiLU(), nn.Linear(width, 1))

    def forward(self, obs):
        if obs['schema'] != SCHEMA:
            raise ValueError('factory.observation-schema')
        device = next(self.parameters()).device
        def tensor(name, dtype=torch.float32):
            return torch.as_tensor(obs[name], dtype=dtype, device=device)
        raster = self.raster(tensor('raster').unsqueeze(0))[0]
        nxy, axy = tensor('node_xy', torch.long), tensor('action_xy', torch.long)
        h = torch.nn.functional.silu(self.node(torch.cat((tensor('nodes'), raster[:, nxy[:, 1], nxy[:, 0]].T), dim=1)))
        edges = tensor('edges', torch.long)
        edge_h = self.edge(tensor('edge_values'))
        degree = torch.zeros(len(h), 1, device=device).index_add_(0, edges[:, 1], torch.ones(len(edges), 1, device=device)).clamp_min(1)
        for message, update, norm in zip(self.messages, self.updates, self.norms):
            aggregate = torch.zeros_like(h).index_add_(0, edges[:, 1], torch.nn.functional.silu(message(h[edges[:, 0]])+edge_h))/degree
            h = norm(h + torch.nn.functional.silu(update(torch.cat((h, aggregate), dim=1))))
        operands = tensor('operands', torch.long)
        operands_h = torch.cat((torch.zeros(1, self.width, device=device), h))[operands].flatten(1)
        global_h = torch.cat((h.mean(0), raster.mean((1, 2)), tensor('context'))).expand(len(operands), -1)
        member_actions = tensor('member_actions', torch.long)
        member_nodes, member_xy = tensor('member_nodes', torch.long), tensor('member_xy', torch.long)
        counts = torch.zeros(len(operands), 1, device=device).index_add_(0, member_actions, torch.ones(len(member_actions), 1, device=device)).clamp_min(1)
        source = torch.zeros(len(operands), self.width, device=device).index_add_(0, member_actions, h[member_nodes])/counts
        destination = torch.zeros_like(source).index_add_(0, member_actions, raster[:, member_xy[:, 1], member_xy[:, 0]].T)/counts
        return self.score(torch.cat((tensor('actions'), operands_h, raster[:, axy[:, 1], axy[:, 0]].T, source, destination, global_h), dim=1)).squeeze(1)
