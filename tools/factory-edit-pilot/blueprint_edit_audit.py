"""Independent directed-chain audit of an exported witness against its blueprint."""
from collections import defaultdict
from fractions import Fraction


def require(ok, message):
    if not ok:
        raise ValueError(f'blueprint.topology-audit: {message}')


def point(value):
    return value['x'], value['y']


def rate(value):
    return Fraction(value['numerator'], value['denominator'])


def audit(original, blueprint, witness):
    require(witness['schema_version'] == original['schema_version'], 'witness schema changed')
    require(witness['utilizations'] == original['utilizations'], 'utilizations changed')
    require(witness['logistics_components'] == original['logistics_components'], 'component identity/pose changed')
    require(len(witness['placements']) == len(original['placements']), 'facility count changed')
    for before, after in zip(original['placements'], witness['placements'], strict=True):
        for key in ('instance', 'facility', 'recipe'):
            require(before[key] == after[key], f'facility {key} changed')
    require(len(witness['transport_networks']) == len(original['transport_networks']), 'network count changed')
    terminal_maps, outgoing, incoming, segments, visited = [], [], [], [], []
    for before, after in zip(original['transport_networks'], witness['transport_networks'], strict=True):
        for key in ('id', 'requirement_ids', 'item', 'transport', 'component_ids'):
            require(before[key] == after[key], f'network {key} changed')
        require(len(before['terminals']) == len(after['terminals']), 'terminal count changed')
        lookup = {t['id']: t for t in after['terminals']}
        require(len(lookup) == len(after['terminals']), 'duplicate terminal ID')
        for t in before['terminals']:
            require(t['id'] in lookup, f'missing terminal {t["id"]}')
            for key in ('id', 'node', 'direction', 'endpoint', 'rate'):
                require(t[key] == lookup[t['id']][key], f'terminal {t["id"]} {key} changed')
            if t['endpoint']['kind'] == 'external':
                require(t['position'] == lookup[t['id']]['position'], 'external anchor moved')
        terminal_maps.append(lookup)
        outs, ins = defaultdict(list), defaultdict(list)
        for i, segment in enumerate(after['segments']):
            outs[point(segment['from'])].append(i)
            ins[point(segment['to'])].append(i)
        outgoing.append(outs)
        incoming.append(ins)
        segments.append(after['segments'])
        visited.append(set())
    positions = []
    for node in blueprint['nodes']:
        terminal = node['terminal']
        positions.append(point(terminal_maps[node['network']][terminal['id']]['position']) if terminal else point(node['initial_position']))
    node_sites = defaultdict(set)
    for node in blueprint['nodes']:
        node_sites[node['network']].add(positions[node['id']])
    direction = {(1, 0): 0, (0, 1): 1, (-1, 0): 2, (0, -1): 3}
    original_poses = {p['instance']: p for p in original['placements']}
    final_poses = {p['instance']: p for p in witness['placements']}
    def oriented(edge, endpoint, axis):
        terminal = blueprint['nodes'][edge[endpoint]]['terminal']
        if axis is not None and terminal and terminal['endpoint']['kind'] == 'facility':
            instance = terminal['endpoint']['instance']
            axis = (axis + (final_poses[instance]['rotation'] - original_poses[instance]['rotation']) // 90) % 4
        return axis
    def segment_direction(segment):
        a, b = point(segment['from']), point(segment['to'])
        return direction.get((b[0] - a[0], b[1] - a[1]))
    lengths = []
    used_cells = [set() for _ in segments]
    for edge in blueprint['edges']:
        network = edge['network']
        start, target = positions[edge['source']], positions[edge['target']]
        if start == target:
            require(edge['zero_length'], 'distinct original anchors collapsed')
            used_cells[network].add(start)
            lengths.append(0)
            continue
        current, first, count = start, oriented(edge, 'source', edge['first']), 0
        seen = {start}
        while current != target:
            choices = [i for i in outgoing[network][current]
                       if rate(segments[network][i]['rate']) == rate(edge['rate'])
                       and (count != 0 or first is None or segment_direction(segments[network][i]) == first)]
            require(len(choices) == 1, f'edge {edge["id"]} has changed incidence at {current}')
            index = choices[0]
            require(index not in visited[network], 'two original edges share a physical segment')
            visited[network].add(index)
            segment = segments[network][index]
            axis = segment_direction(segment)
            require(axis is not None, 'non-unit physical segment')
            current = point(segment['to'])
            require(current not in seen, 'route cycle introduced')
            seen.add(current)
            count += 1
            if current == target:
                last = oriented(edge, 'target', edge['last'])
                require(last is None or axis == last, 'original component/terminal incoming direction changed')
            else:
                require(current not in node_sites[network], 'edge reaches a different original anchor')
                require(len(incoming[network][current]) == len(outgoing[network][current]) == 1, 'new merge or branch introduced')
        used_cells[network].update(seen)
        lengths.append(count)
    require(all(len(ids) == len(rows) for ids, rows in zip(visited, segments)), 'unowned physical segments remain')
    for network, cells in zip(witness['transport_networks'], used_cells):
        require(set(map(point, network['cells'])) == cells, 'unowned or missing physical cells')
    return dict(original_directed_edges=len(blueprint['edges']), exact_incidence=True,
                exact_terminal_metadata=True, exact_component_identity_and_pose=True,
                segment_count=sum(lengths), edge_lengths=lengths)
