"""Immutable graph tensors and catalog-derived declarative decision domains."""
import math

import torch

from blueprint_edit_input import fingerprint, rate
from blueprint_ppo_update import require

SCHEMA = 'declarative-learning-graph-v1'
PACKED = 'declarative-learning-packed-v1'
KINDS = ('place', 'insert_band', 'delete_band')
PORT_FEATURES = 12


class Catalog:
    """Read once per worker; only immutable catalog geometry is cached."""
    def __init__(self, value):
        self.facilities = {f['id']: f for f in value['facilities']}
        self.rotated = {}
        directions = ('north', 'east', 'south', 'west')
        for facility in self.facilities.values():
            width, height = (facility['footprint'][k] for k in ('width', 'height'))
            for rotation in facility['allowed_rotations']:
                ports = {}
                for port in facility['ports']:
                    x, y = port['position']['x'], port['position']['y']
                    w, h = width, height
                    direction = directions.index(port['edge'])
                    for _ in range(rotation // 90):
                        x, y, w, h, direction = h - 1 - y, x, h, w, (direction + 1) % 4
                    dx, dy = ((0, -1), (1, 0), (0, 1), (-1, 0))[direction]
                    ports[port['id']] = dict(x=x + dx, y=y + dy, dx=dx, dy=dy,
                        direction=port['direction'], transport=port['transport'])
                self.rotated[(facility['id'], rotation)] = ports


def tensor(rows, columns, dtype=torch.float32):
    return torch.tensor(rows, dtype=dtype).reshape(-1, columns)


def prepare(observation, catalog, config):
    require(observation['schema'] == 'declarative-edit-observation-v1', 'declarative observation schema')
    bp, facilities = observation['blueprint'], observation['facilities']
    facility_index = {f['instance']: i for i, f in enumerate(facilities)}
    offset = len(facilities)
    xs = [n['position']['x'] for n in observation['nodes']]
    ys = [n['position']['y'] for n in observation['nodes']]
    for f in facilities:
        xs.extend((f['x'], f['x'] + f['width'] - 1))
        ys.extend((f['y'], f['y'] + f['height'] - 1))
    for edge in observation['edges']:
        for point in edge['path'] or []:
            xs.append(point['x']); ys.append(point['y'])
    scale = max(1, max(xs) + 1, max(ys) + 1)
    collisions = {f['instance']: 0. for f in facilities}
    for conflict in observation['conflicts']:
        for name in conflict.get('facilities', [conflict.get('facility')]):
            collisions[name] += 1
    static, state = [], []
    for f, original in zip(facilities, bp['facilities'], strict=True):
        require(f['instance'] == original['instance'], 'declarative facility identity order')
        static.append([1., 0., 0., 0., original['width'] / scale, original['height'] / scale,
                       rate(bp['utilizations'][f['instance']]), 0., 0., 0., *fingerprint(f['recipe'])])
        angle = math.radians(f['rotation'])
        state.append([f['x'] / scale, f['y'] / scale, f['width'] / scale, f['height'] / scale,
                      math.sin(angle), math.cos(angle), collisions[f['instance']], 0., 0., 1.])
    for node, current in zip(bp['nodes'], observation['nodes'], strict=True):
        require(node['id'] == current['id'], 'declarative anchor identity order')
        terminal = node['terminal']
        external = terminal is not None and terminal['endpoint']['kind'] == 'external'
        network = bp['networks'][node['network']]
        static.append([0., float(terminal is not None), float(bool(node['components'])), float(external),
                       0., 0., 0., (1. if terminal['direction'] == 'output' else -1.) if terminal else 0.,
                       rate(terminal['rate']) if terminal else 0., float(network['transport'] == 'pipe'),
                       *fingerprint(network['item'])])
        dx = dy = 0.
        if terminal and terminal['endpoint']['kind'] == 'facility':
            f = facilities[facility_index[terminal['endpoint']['instance']]]
            port = catalog.rotated[(f['facility'], f['rotation'])][observation['port_assignments'][node['id']]]
            dx, dy = port['dx'], port['dy']
        state.append([current['position']['x'] / scale, current['position']['y'] / scale,
                      0., 0., dx, dy, 0., 0., 0., 1.])
    links, edge_static, edge_state, path_cells, path_edges = [], [], [], [], []
    for edge, current in zip(bp['edges'], observation['edges'], strict=True):
        require(edge['id'] == current['id'], 'declarative edge identity order')
        network = bp['networks'][edge['network']]
        links.append([offset + edge['source'], offset + edge['target']])
        edge_static.append([1., 0., float(network['transport'] == 'pipe'), rate(edge['rate']),
                            (edge['first'] + 1) / 4 if edge['first'] is not None else 0.,
                            (edge['last'] + 1) / 4 if edge['last'] is not None else 0., *fingerprint(network['item'])])
        source, target = (observation['nodes'][edge[k]]['position'] for k in ('source', 'target'))
        edge_state.append([float(current['status'] == 'pending'),
                           (len(current['path']) - 1) / scale if current['path'] else 0.,
                           (target['x'] - source['x']) / scale, (target['y'] - source['y']) / scale])
        for point in current['path'] or []:
            path_cells.append([point['x'] / scale, point['y'] / scale]); path_edges.append(edge['id'])
    for node in bp['nodes']:
        terminal = node['terminal']
        if terminal and terminal['endpoint']['kind'] == 'facility':
            links.append([facility_index[terminal['endpoint']['instance']], offset + node['id']])
            edge_static.append([0., 1., 0., 0., 0., 0., *fingerprint(observation['port_assignments'][node['id']])])
            edge_state.append([0.] * 4)

    actions, features, references, owners = [], [], [], []
    def add(action, owner, extra):
        actions.append(action); owners.append(owner)
        features.append([float(action['kind'] == kind) for kind in KINDS] + extra)
        references.append([owner, owner])
    for i, facility in enumerate(facilities):
        destinations = {(facility['x'], facility['y'])}
        for step in config['translation_steps']:
            destinations.update(((facility['x'] + step, facility['y']), (facility['x'], facility['y'] + step)))
        destinations.update((other['x'], other['y']) for other in facilities)
        for x, y in sorted(destinations):
            if min(x, y) < 0:
                continue
            for rotation in observation['allowed_rotations'][facility['instance']]:
                definition = catalog.facilities[facility['facility']]['footprint']
                w, h = definition['width'], definition['height']
                if rotation % 180:
                    w, h = h, w
                angle = math.radians(rotation)
                add(dict(kind='place', instance=facility['instance'], x=x, y=y, rotation=rotation), i,
                    [(x-facility['x'])/scale, (y-facility['y'])/scale, math.sin(angle), math.cos(angle),
                     w/scale, h/scale, 0., 0., 0.])
    for kind in KINDS[1:]:
        for axis, points in [('x', xs), ('y', ys)]:
            for at in range(min(points), max(points) + 1):
                for count in config['band_counts']:
                    add(dict(kind=kind, axis=axis, at=at, count=count), -1,
                        [0., 0., 0., 1., 0., 0., float(axis == 'y'), at/scale, count/scale])
    groups = []
    for facility in facilities:
        rows = []
        for option in sorted((o for o in observation['port_options'] if o['instance'] == facility['instance']), key=lambda o:o['node']):
            by_rotation = {}
            for rotation in observation['allowed_rotations'][facility['instance']]:
                choices = catalog.rotated[(facility['facility'], rotation)]
                values = []
                for name in option['ports']:
                    port = choices[name]
                    values.append([port['x']/scale, port['y']/scale, port['dx'], port['dy'],
                        float(port['direction'] == 'output'), float(port['transport'] == 'pipe'),
                        float(name == observation['port_assignments'][option['node']]), *fingerprint(name), 1.])
                by_rotation[rotation] = tensor(values, PORT_FEATURES)
            rows.append(dict(node=option['node'], graph_node=offset+option['node'], ports=option['ports'], features=by_rotation))
        groups.append(rows)
    initial, best = observation['initial_metrics']['score'], observation['best_metrics']['score']
    global_features = [b/max(a, 1) for b, a in zip(best, initial)]
    global_features += [sum(e['status'] == 'pending' for e in observation['edges'])/max(len(bp['edges']),1),
        sum(c.get('area',1) for c in observation['conflicts'])/max(initial[0],1),
        observation['elapsed_ms']/observation['config']['elapsed_ms'], observation['attempts']/observation['config']['max_actions']]
    return dict(schema=SCHEMA, blueprint_sha256=observation['blueprint_sha256'], revision=observation['revision'],
        node_static=tensor(static,14), node_state=tensor(state,10), edge_index=tensor(links,2,torch.long).T.contiguous(),
        edge_static=tensor(edge_static,10), edge_state=tensor(edge_state,4), path_cells=tensor(path_cells,2),
        path_edges=torch.tensor(path_edges,dtype=torch.long), global_features=torch.tensor(global_features),
        action_features=tensor(features,12), action_references=tensor(references,2,torch.long),
        actions=actions, root_owners=owners, port_groups=groups)
