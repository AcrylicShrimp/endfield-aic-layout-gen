"""Analyze captured native A* inputs without changing the production router.

The A* replica checks expansion counts, not path reconstruction. The relaxed
reverse graph removes the crossing-count cap and permits revisiting cells. A
missing connection therefore proves only this frozen path query unreachable.
"""
import argparse
from collections import Counter, deque
import heapq
import json
from pathlib import Path

DIRECTIONS = ((1, 0), (0, 1), (-1, 0), (0, -1))


def analyze(data):
    point = lambda p: (p['x'], p['y'])
    start, end = point(data['start']), point(data['end'])
    blocked = {point(p) for p in data['blocked']}
    crossings = {(point(c['position']), c['direction']) for c in data['crossings']}
    b = data['bounds']
    width, height = b['maxX'] - b['minX'] + 1, b['maxY'] - b['minY'] + 1
    limit, first, last = data['crossing_limit'], data['first'], data['last']
    if not (width > 0 and height > 0 and width * height <= 250000
            and 0 <= limit <= 8 and data['work_budget'] > 0
            and first in (None, 0, 1, 2, 3) and last in (None, 0, 1, 2, 3)):
        raise ValueError('expected valid captured native path limits')
    weight = width * height * 4 * (limit + 1) + 1

    def inside(p):
        return b['minX'] <= p[0] <= b['maxX'] and b['minY'] <= p[1] <= b['maxY']

    if start == end or start in blocked or end in blocked or not inside(start) or not inside(end):
        raise ValueError('analyzer requires distinct, in-bounds, unblocked endpoints')

    def successors(p, direction):
        for d, (dx, dy) in enumerate(DIRECTIONS):
            if p in blocked and d != direction:
                continue
            if direction == 4 and first is not None and d != first:
                continue
            if direction != 4 and (d + 2) % 4 == direction:
                continue
            q = (p[0] + dx, p[1] + dy)
            if q in blocked and (q, d) not in crossings:
                continue
            if not inside(q) or q == start or (q == end and last is not None and d != last):
                continue
            yield q, d, int(q in blocked)

    def ident(p, d, used):
        return (((p[1] - b['minY']) * width + p[0] - b['minX']) * 5 + d) * (limit + 1) + used

    def distance(p):
        return abs(p[0] - end[0]) + abs(p[1] - end[1])

    heap = [(distance(start) * weight, 0, ident(start, 4, 0), 0, start, 4, 0)]
    best = {(start, 4, 0): 0}
    expanded = []
    serial = 0
    status = 'no-path'
    while heap:
        _, g, _, _, p, d, used = heapq.heappop(heap)
        if best[p, d, used] != g:
            continue
        if len(expanded) == data['work_budget']:
            status = 'work-budget'
            break
        expanded.append((p, d, used))
        if p == end and (last is None or last == d):
            status = 'found'
            break
        for q, nd, crossing in successors(p, d):
            nu = used + crossing
            if nu > limit:
                continue
            ng = g + weight + int(d != 4 and d != nd)
            state = (q, nd, nu)
            if best.get(state, ng + 1) <= ng:
                continue
            best[state] = ng
            serial += 1
            heapq.heappush(heap, (ng + distance(q) * weight, ng, ident(q, nd, nu), serial, q, nd, nu))

    # Materialize a small diagnostic relaxation. Production need not eagerly
    # build this graph; a reverse distance field is a separate implementation.
    reverse = {}
    nodes = [(start, 4)]
    for y in range(b['minY'], b['maxY'] + 1):
        for x in range(b['minX'], b['maxX'] + 1):
            p = (x, y)
            nodes.extend((p, d) for d in range(4)
                         if p != start and (p not in blocked or (p, d) in crossings))
    for p, d in nodes:
        for q, nd, _ in successors(p, d):
            reverse.setdefault((q, nd), []).append((p, d))
    reachable = {(end, d) for d in range(4) if last is None or d == last}
    queue = deque(reachable)
    while queue:
        for previous in reverse.get(queue.popleft(), []):
            if previous not in reachable:
                reachable.add(previous)
                queue.append(previous)
    return dict(status=status, expanded=len(expanded),
                unique_cells=len({p for p, _, _ in expanded}),
                unique_oriented_cells=len({(p, d) for p, d, _ in expanded}),
                expansions_by_crossing_count=dict(sorted(Counter(u for _, _, u in expanded).items())),
                relaxed_start_reaches_end=(start, 4) in reachable,
                relaxed_reverse_states=len(reachable),
                relaxed_reverse_cells=sorted({p for p, _ in reachable}),
                bounds_area=width * height)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inputs', type=Path, nargs='+')
    args = parser.parse_args()
    for path in args.inputs:
        try:
            result = analyze(json.loads(path.read_text()))
        except (OSError, KeyError, TypeError, ValueError) as error:
            parser.error(f'{path}: {error}')
        print(json.dumps(dict(input=str(path), **result)))


if __name__ == '__main__':
    main()
