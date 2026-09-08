"""Independent directional reachability counterfactual for a recorded routing trial.

This proves grid reachability only, not flow feasibility or factory validity.
"""
import argparse
from collections import deque
from fractions import Fraction
import json
from pathlib import Path
from constructor_attachment_trace import read, sha


def xy(p):
    return p['x'], p['y']


def rate(value):
    return Fraction(value['numerator'], value['denominator'])


def route(width, height, blocked, crossings, start, goal):
    inside = lambda p: 0 <= p[0] < width and 0 <= p[1] < height
    if any(not inside(p) or p in blocked or p in crossings for p in (start, goal)):
        return None
    initial = (*start, None)
    parents = {initial: None}
    queue = deque([initial])
    while queue:
        state = queue.popleft()
        x, y, heading = state
        if (x, y) == goal:
            path = []
            while state is not None:
                path.append(list(state[:2]))
                state = parents[state]
            return path[::-1]
        for direction in ((0, -1), (1, 0), (0, 1), (-1, 0)):
            dx, dy = direction
            horizontal = dy == 0
            if (x, y) in crossings and (crossings[x, y] != horizontal or heading != direction):
                continue
            p = (x+dx, y+dy)
            if not inside(p) or (p in blocked and p not in crossings):
                continue
            if p in crossings and crossings[p] != horizontal:
                continue
            following = (*p, direction)
            if following not in parents:
                parents[following] = state
                queue.append(following)
    return None


def same_item_options(snapshot, catalog):
    edge = snapshot['edge']
    transport = snapshot['trial']['source']['attachment']['port']['transport']
    needed = rate(edge['rate'])
    definitions = [c for c in catalog['components'] if c['kind'] == 'bridge'
        and c['transport'] == transport and c['footprint'] == {'width': 1, 'height': 1}
        and len(c['input_directions']) == len(c['output_directions']) == 4
        and Fraction(c['capacity']['quantity']*1000, c['capacity']['duration_ms']) >= needed]
    if not definitions:
        return {}, []
    definition = definitions[0]
    capacity = Fraction(definition['capacity']['quantity']*1000, definition['capacity']['duration_ms'])
    occupied = {xy(c['position']) for c in snapshot['node']['logistics_components'] if c['transport'] == transport}
    choices = {}; evidence = []
    for network in snapshot['node']['transport_networks']:
        if network['transport'] != transport or network['item'] != edge['item']:
            continue
        terminals = {xy(t['position']) for t in network['terminals']}
        for cell in network['cells']:
            p = xy(cell)
            if p in occupied or p in terminals:
                continue
            incoming = [s for s in network['segments'] if xy(s['to']) == p]
            outgoing = [s for s in network['segments'] if xy(s['from']) == p]
            if len(incoming) != 1 or len(outgoing) != 1:
                continue
            a, b = xy(incoming[0]['from']), xy(outgoing[0]['to'])
            delta = p[0]-a[0], p[1]-a[1]
            if abs(delta[0])+abs(delta[1]) != 1 or (b[0]-p[0], b[1]-p[1]) != delta:
                continue
            load = rate(incoming[0]['rate'])
            if load != rate(outgoing[0]['rate']) or load > capacity:
                continue
            choices[p] = delta[0] == 0
            evidence.append(dict(position=list(p), horizontal=choices[p], component=definition['id'],
                existing_rate=str(load), proposed_rate=str(needed), per_axis_capacity=str(capacity)))
    return choices, evidence


def run(args):
    snapshot = read(args.snapshot)
    trial = snapshot['trial']
    start, goal = [xy(trial[side]['endpoint']) for side in ('source', 'target')]
    blocked = {tuple(p) for p in snapshot['blocked']}
    physical = {tuple(p) for key in ('facility_cells', 'layer_cells') for p in snapshot[key]}
    crossings = {(p['x'], p['y']): p['horizontal'] for p in snapshot['crossings']}
    extra, evidence = same_item_options(snapshot, read(args.catalog))
    paths = {name: route(trial['width'], trial['height'], mask, options, start, goal)
        for name, mask, options in [('actual', blocked, crossings),
            ('without_reservations', physical, crossings),
            ('same_item_candidates', blocked, crossings | extra)]}
    report = dict(schema_version=1, scope='Directional grid reachability only; no validated factory claim',
        source_sha256={str(p.resolve()): sha(p) for p in (args.snapshot, args.catalog, Path(__file__))},
        paths=paths, same_item_options=evidence,
        used_same_item_crossings=[e for e in evidence if e['position'] in (paths['same_item_candidates'] or [])])
    with args.output.open('x') as output:
        output.write(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(paths=paths, extra_candidates=len(extra), used=report['used_same_item_crossings'])))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('snapshot', 'catalog', 'output'):
        parser.add_argument('--'+name, required=True, type=Path)
    run(parser.parse_args())
