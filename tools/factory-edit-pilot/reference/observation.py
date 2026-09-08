"""Schema-2 semantic entities and operands, independent of instance cardinality."""

import math
import time

import numpy as np

from .acceptance import DIRECTIONS, SIDES
from .commands import CandidateSet

SCHEMA_VERSION = 2
ENTITY_FEATURES = 42
CANDIDATE_FEATURES = 15
OPERAND_FEATURES = 15
RASTER_CHANNELS = 23
OPERATIONS = ('SET_PLACEMENTS', 'UNBIND_TERMINALS', 'BIND_TERMINALS', 'ERASE_TRANSPORT',
              'EXTEND', 'CONNECT', 'DISCARD_EDIT')
ARMS = tuple(DIRECTIONS.values())


def prepare(env):
    nf, nn, nt = len(env.fids), len(env.nids), len(env.tids)
    env.entity_count = nf+nn+nt
    env.entity_ids = {**{('facility', v): i for i, v in enumerate(env.fids)},
                      **{('network', v): nf+i for i, v in enumerate(env.nids)},
                      **{('terminal', v): nf+nn+i for i, v in enumerate(env.tids)}}
    template = np.zeros((2*env.entity_count, ENTITY_FEATURES), dtype=np.float32)
    edges, types = [], []
    for snapshot in range(2):
        offset = snapshot*env.entity_count
        template[offset:offset+env.entity_count, 3+snapshot] = 1
        for fid in env.fids:
            row = template[offset+env.entity_ids['facility', fid]]
            definition = env.catalog.facility_type(fid)
            row[0] = 1
            for rotation in definition['allowed_rotations']:
                row[13+rotation//90] = 1
            row[27] = env.catalog.facilities[fid]['utilization'][0]/env.catalog.facilities[fid]['utilization'][1]
            terminals = [env.terminal_metadata[t] for t in env.tids if env.terminal_metadata[t][2].get('facility') == fid]
            row[28:30] = [math.log1p(sum(t[1] == role for t in terminals)) for role in ('target', 'source')]
            row[30:34] = [math.log1p(sum(p['direction'] == direction and p['transport'] == transport
                                       for p in definition['ports']))
                          for transport in ('belt', 'pipe') for direction in ('input', 'output')]
        for nid in env.nids:
            row = template[offset+env.entity_ids['network', nid]]
            row[1] = row[34] = 1
            row[25] = float(env.catalog.rate(nid)/env.catalog.capacity)
            row[26] = math.log1p(float(env.catalog.capacity))
            row[28:30] = math.log1p(1)
        for tid in env.tids:
            nid, role, endpoint = env.terminal_metadata[tid]
            terminal = offset+env.entity_ids['terminal', tid]
            network = offset+env.entity_ids['network', nid]
            row = template[terminal]
            row[2] = row[17 if role == 'source' else 18] = row[34] = 1
            row[19] = 'external' in endpoint
            row[25] = float(env.catalog.rate(nid)/env.catalog.capacity)
            row[26] = math.log1p(float(env.catalog.capacity))
            if 'facility' in endpoint:
                owner = offset+env.entity_ids['facility', endpoint['facility']]
                edges.extend([(owner, terminal), (terminal, owner)])
                types.extend([0, 1])
            edges.extend([(terminal, network), (network, terminal)])
            types.extend([2, 3] if role == 'source' else [4, 5])
    for i in range(env.entity_count):
        edges.extend([(i, env.entity_count+i), (env.entity_count+i, i)])
        types.extend([6, 7])
    template.flags.writeable = False
    env.entity_template = template
    env.graph_edges = np.asarray(edges, dtype=np.int64)
    env.graph_edge_types = np.asarray(types, dtype=np.int64)


def command_context(env, command):
    if command.op == 'UNBIND_TERMINALS':
        return env.location(env.draft, command.args[0])
    if command.op == 'BIND_TERMINALS':
        return env.location(env.draft, command.args[0], command.to_dict()['binding'])
    return None


def encode_candidate(env, command, context):
    op, args = command
    feature = np.zeros(CANDIDATE_FEATURES, dtype=np.float32)
    feature[OPERATIONS.index(op)] = 1
    operands = np.zeros((2, OPERAND_FEATURES), dtype=np.float32)
    entities = np.full(2, -1, dtype=np.int64)
    points = np.zeros((2, 2), dtype=np.int64)
    mask = np.zeros(2, dtype=bool)

    def operand(slot, kind, identifier, role, point, rotation=None, arm=None):
        entities[slot] = env.entity_ids[kind, identifier]
        mask[slot] = True
        operands[slot, role] = 1
        operands[slot, 5:7] = point[0]/env.scale, point[1]/env.scale
        if rotation is not None:
            operands[slot, 7+rotation//90] = 1
        if arm is not None:
            operands[slot, 11+ARMS.index(tuple(arm))] = 1
        points[slot] = point

    if op == 'SET_PLACEMENTS':
        feature[14] = len(args)
        for i, (fid, pose) in enumerate(args):
            operand(i, 'facility', fid, 0, pose[:2], rotation=pose[2])
    elif op in ('BIND_TERMINALS', 'UNBIND_TERMINALS'):
        operand(0, 'terminal', args[0], 4, context[0], arm=context[1])
    elif op == 'ERASE_TRANSPORT':
        feature[13] = math.log1p(len(args[1]))
        for i, cell in enumerate(args[1]):
            operand(i, 'network', args[0], 3, cell)
    elif op == 'EXTEND':
        nid, anchor, growth, direction, length = args
        dx, dy = DIRECTIONS[direction]
        feature[7+SIDES.index(direction)] = 1
        feature[11+int(growth == 'backward')] = 1
        feature[13] = math.log1p(length)
        operand(0, 'network', nid, 1, anchor)
        operand(1, 'network', nid, 2, (anchor[0]+dx*length, anchor[1]+dy*length))
    elif op == 'CONNECT':
        operand(0, 'network', args[0], 1, args[1])
        operand(1, 'network', args[0], 2, args[2])
    elif op != 'DISCARD_EDIT':
        raise ValueError(f'observation.command: unsupported operation {op}')
    for value in (feature, operands, entities, points, mask):
        value.flags.writeable = False
    return feature, operands, entities, points, mask


def observe(env, candidates):
    from .environment import Mechanical, SpatialGuard
    started = time.perf_counter()
    if not isinstance(candidates, CandidateSet) or candidates.state is not env.draft or candidates.revision != env.revision:
        raise Mechanical('observation.stale_candidates: regenerate candidates for the current draft')
    indexes = [candidates.index, env.index(env.base)]
    all_points = indexes[0][1] | indexes[1][1]
    xs, ys = zip(*all_points)
    halo = env.config['halo']
    x0, y0, x1, y1 = min(xs)-halo, min(ys)-halo, max(xs)+halo, max(ys)+halo
    width, height = x1-x0+1, y1-y0+1
    if width*height > env.config['raster_cell_guard']:
        raise SpatialGuard(f'observation.resource_guard: {width}x{height} exceeds declared raster-work guard')
    raster = np.zeros((RASTER_CHANNELS, height, width), dtype=np.float32)
    maps = np.full((6, height, width), -1, dtype=np.int64)
    entities = env.entity_template.copy()
    for snapshot, (state, index) in enumerate(zip((env.draft, env.base), indexes)):
        offset = snapshot*env.entity_count
        channel = snapshot*10
        bodies, _, incoming, outgoing, _, _ = index
        for fid in env.fids:
            eid = offset+env.entity_ids['facility', fid]
            row = entities[eid]
            pose = state['placements'][fid]
            definition = env.catalog.facility_type(fid)
            w, h = definition['footprint']['width'], definition['footprint']['height']
            if pose[2] % 180:
                w, h = h, w
            row[5:9] = pose[0]/env.scale, pose[1]/env.scale, w/env.scale, h/env.scale
            row[9+pose[2]//90] = 1
            row[36] = math.log1p(len(bodies[fid]))
            incident = [t for t in env.tids if env.terminal_metadata[t][2].get('facility') == fid]
            row[37] = sum(state['bindings'][t] is not None for t in incident)/max(1, len(incident))
            for x, y in bodies[fid]:
                raster[channel, y-y0, x-x0] = 1
                maps[snapshot*3, y-y0, x-x0] = eid
        for nid in env.nids:
            eid = offset+env.entity_ids['network', nid]
            row = entities[eid]
            cells = state['networks'][nid]['cells']
            if cells:
                xx, yy = zip(*cells)
                row[5:9] = min(xx)/env.scale, min(yy)/env.scale, (max(xx)-min(xx)+1)/env.scale, (max(yy)-min(yy)+1)/env.scale
            row[36] = math.log1p(len(cells))
            row[37] = sum(state['bindings'][f'{nid}:{role}'] is not None for role in ('source', 'target'))/2
            for role, column in (('source', 38), ('target', 40)):
                location = env.location(state, f'{nid}:{role}')
                if location:
                    row[column:column+2] = location[0][0]/env.scale, location[0][1]/env.scale
            for x, y in cells:
                raster[channel+1, y-y0, x-x0] = 1
                maps[snapshot*3+1, y-y0, x-x0] = eid
                for io, arms in enumerate((incoming[nid][x, y], outgoing[nid][x, y])):
                    for arm in arms:
                        raster[channel+2+io*4+ARMS.index(arm), y-y0, x-x0] = 1
        for tid in env.tids:
            eid = offset+env.entity_ids['terminal', tid]
            row = entities[eid]
            location = env.location(state, tid)
            row[20] = state['bindings'][tid] is not None
            if location:
                (x, y), arm = location
                row[5:7] = x/env.scale, y/env.scale
                row[21+ARMS.index(arm)] = 1
                maps[snapshot*3+2, y-y0, x-x0] = eid
    raster[-3] = np.arange(x0, x1+1, dtype=np.float32)[None, :]/env.scale
    raster[-2] = np.arange(y0, y1+1, dtype=np.float32)[:, None]/env.scale
    raster[-1] = 1
    encoded = [env.encode_candidate(command, env.command_context(command)) for command in candidates.commands]
    features, operands, operand_entities, points, masks = zip(*encoded)
    points = np.asarray(points, dtype=np.int64)
    active = np.asarray(masks, dtype=bool)
    points[active] -= np.asarray([x0, y0], dtype=np.int64)
    points[~active] = 0
    stats = [(env.config['horizon']-env.steps)/env.config['horizon'],
             env.best_score[0]/env.initial_score[0], env.base_score[0]/env.initial_score[0],
             env.best_score[1]/env.initial_score[0], env.base_score[1]/env.initial_score[0],
             env.best_score[2]/env.initial_score[0], env.base_score[2]/env.initial_score[0],
             sum(b is None for b in env.draft['bindings'].values())/len(env.tids)]
    cache = env.encode_candidate.cache_info()
    env.counters['encoding_cache_hits'] = cache.hits
    env.counters['encoding_cache_misses'] = cache.misses
    env.counters['encoding_cache_entries'] = cache.currsize
    env.seconds['observation'] += time.perf_counter()-started
    return {'schema_version': SCHEMA_VERSION, 'raster': raster, 'entity_maps': maps, 'entities': entities,
            'edges': env.graph_edges.copy(), 'edge_types': env.graph_edge_types.copy(),
            'stats': np.asarray(stats, dtype=np.float32), 'candidates': np.asarray(features, dtype=np.float32),
            'operands': np.asarray(operands, dtype=np.float32), 'operand_entities': np.asarray(operand_entities, dtype=np.int64),
            'operand_mask': np.asarray(masks, dtype=bool), 'points': points}
