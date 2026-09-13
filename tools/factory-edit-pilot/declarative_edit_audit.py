"""Cold directed-incidence audit; selected ports and grid-anchor positions may change."""
from collections import defaultdict

from blueprint_edit_audit import point, rate, require


def audit(original, blueprint, witness):
    require(witness['schema_version'] == original['schema_version'], 'witness schema changed')
    require(witness['utilizations'] == original['utilizations'], 'utilization changed')
    require(len(witness['placements']) == len(original['placements']), 'facility count changed')
    for before, after in zip(original['placements'], witness['placements'], strict=True):
        for key in ('instance', 'facility', 'recipe'):
            require(before[key] == after[key], f'facility {key} changed')
    components = {}
    for before, after in zip(original['logistics_components'], witness['logistics_components'], strict=True):
        for key in ('id', 'component', 'kind', 'transport', 'rotation'):
            require(before[key] == after[key], f'component {key} changed')
        require(after['id'] not in components, 'duplicate component ID')
        components[after['id']] = point(after['position'])
    terminals, incoming, outgoing, segments, visited = [], [], [], [], []
    changed_ports = 0
    for before, after in zip(original['transport_networks'], witness['transport_networks'], strict=True):
        for key in ('id', 'requirement_ids', 'item', 'transport', 'component_ids'):
            require(before[key] == after[key], f'network {key} changed')
        require(len(before['terminals']) == len(after['terminals']), 'terminal count changed')
        lookup = {t['id']: t for t in after['terminals']}
        require(len(lookup) == len(after['terminals']), 'duplicate terminal ID')
        for t in before['terminals']:
            new = lookup.get(t['id'])
            require(new is not None, 'missing original terminal')
            for key in ('id', 'node', 'direction', 'rate'):
                require(t[key] == new[key], f'terminal {key} changed')
            old_endpoint, new_endpoint = t['endpoint'], new['endpoint']
            if old_endpoint['kind'] == 'facility':
                require(new_endpoint['kind'] == 'facility' and new_endpoint['instance'] == old_endpoint['instance'],
                        'facility endpoint rewired')
                changed_ports += old_endpoint['port'] != new_endpoint['port']
            else:
                require(old_endpoint == new_endpoint, 'external obligation changed')
        terminals.append(lookup)
        outs, ins = defaultdict(list), defaultdict(list)
        for index, segment in enumerate(after['segments']):
            outs[point(segment['from'])].append(index)
            ins[point(segment['to'])].append(index)
        outgoing.append(outs)
        incoming.append(ins)
        segments.append(after['segments'])
        visited.append(set())
    positions = []
    for node in blueprint['nodes']:
        if node['terminal']:
            positions.append(point(terminals[node['network']][node['terminal']['id']]['position']))
        elif node['components']:
            sites = {components[c] for c in node['components']}
            require(len(sites) == 1, 'shared component anchor split')
            positions.append(next(iter(sites)))
        else:
            # Current admitted chains have terminals or identified components at
            # anchors. Do not invent correspondence for an anonymous moved node.
            raise ValueError(f'declarative.audit-anonymous-anchor: {node["id"]}')
    node_sites = defaultdict(set)
    for node, position in zip(blueprint['nodes'], positions, strict=True):
        node_sites[node['network']].add(position)
    direction = {(1, 0): 0, (0, 1): 1, (-1, 0): 2, (0, -1): 3}
    original_poses = {p['instance']: p for p in original['placements']}
    final_poses = {p['instance']: p for p in witness['placements']}

    def oriented(edge, endpoint, axis):
        node = blueprint['nodes'][edge[endpoint]]
        terminal = node['terminal']
        if terminal and terminal['endpoint']['kind'] == 'facility':
            old = terminal['endpoint']
            new = terminals[node['network']][terminal['id']]['endpoint']
            if old['port'] != new['port']:
                return None
            if axis is not None:
                axis = (axis + (final_poses[old['instance']]['rotation'] - original_poses[old['instance']]['rotation']) // 90) % 4
        return axis

    def heading(segment):
        a, b = point(segment['from']), point(segment['to'])
        return direction.get((b[0] - a[0], b[1] - a[1]))

    lengths, used_cells = [], [set() for _ in segments]
    for edge in blueprint['edges']:
        network = edge['network']
        start, target = positions[edge['source']], positions[edge['target']]
        if start == target:
            require(edge['zero_length'], 'distinct original anchors collapsed')
            used_cells[network].add(start)
            lengths.append(0)
            continue
        current, first, count, seen = start, oriented(edge, 'source', edge['first']), 0, {start}
        while current != target:
            choices = [i for i in outgoing[network][current] if rate(segments[network][i]['rate']) == rate(edge['rate'])
                       and (count != 0 or first is None or heading(segments[network][i]) == first)]
            require(len(choices) == 1, f'original edge {edge["id"]} has changed incidence at {current}')
            index = choices[0]
            require(index not in visited[network], 'original edges share a physical segment')
            visited[network].add(index)
            segment = segments[network][index]
            axis = heading(segment)
            require(axis is not None, 'non-unit segment')
            current = point(segment['to'])
            require(current not in seen, 'new cycle')
            seen.add(current)
            count += 1
            if current == target:
                last = oriented(edge, 'target', edge['last'])
                require(last is None or axis == last, 'original component/retained-port heading changed')
            else:
                require(current not in node_sites[network], 'route reaches another original anchor')
                require(len(incoming[network][current]) == len(outgoing[network][current]) == 1, 'new split or merge')
        used_cells[network].update(seen)
        lengths.append(count)
    require(all(len(ids) == len(rows) for ids, rows in zip(visited, segments)), 'unowned physical segment')
    for network, cells in zip(witness['transport_networks'], used_cells):
        require(set(map(point, network['cells'])) == cells, 'unowned or missing cells')
    return dict(exact_directed_incidence=True, exact_rates_and_obligations=True,
                exact_component_identity_and_channels=True, changed_ports=changed_ports, segment_count=sum(lengths))
