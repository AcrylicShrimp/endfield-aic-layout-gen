"""Registered kind-balanced draws; geometry validity is determined only by the native editor."""
import random

from declarative_action_runtime import assignments, check


def port_choices(observation, instance, rng):
    options = sorted((o for o in observation['port_options'] if o['instance'] == instance), key=lambda o:o['node'])
    selected, used = [], set()
    # Catalog domains are identical within each direction/transport group. A
    # randomized without-replacement draw therefore supplies a complete joint map.
    for row in options:
        ports = [p for p in row['ports'] if p not in used]
        check(bool(ports), 'port-domain', 'Compatible joint port assignment exhausted', row['node'])
        port = rng.choice(ports)
        used.add(port)
        selected.append(dict(node=row['node'], port=port))
    return selected


def band(observation, rng, delete, options):
    axis = rng.choice(('x','y'))
    points = [n['position'][axis] for n in observation['nodes']]
    extent = 'width' if axis == 'x' else 'height'
    for p in observation['facilities']:
        points.extend((p[axis], p[axis]+p[extent]-1))
    for edge in observation['edges']:
        if edge['path']:
            points.extend(p[axis] for p in edge['path'])
    return dict(kind='delete_band' if delete else 'insert_band', axis=axis,
        at=rng.randint(min(points), max(points)), count=rng.choice(options['band_counts']))


def draw(observation, base, arm, seed, index, options):
    # Independent substreams keep common placement draws identical when adding
    # rotation or port choices. No arm consumes another arm's RNG state.
    rng = random.Random(seed + index * 1009)
    geometry = random.Random(seed + index * 1009 + 1000000007)
    extras = random.Random(seed + index * 1009 + 2000000011)
    if arm == 'legacy_random':
        kind = rng.choice(('pose','repair','repair_all','commit','restore_best'))
        if kind == 'repair':
            return dict(kind=kind, edge=rng.randrange(len(base['blueprint']['edges'])))
        if kind != 'pose':
            return dict(kind=kind)
    if arm in ('insert_band','delete_band'):
        return band(observation, extras, arm == 'delete_band', options)
    if arm == 'automatic_grid' and rng.random() >= options['grid_place_probability']:
        return band(observation, extras, rng.randrange(2) == 1, options)
    facilities = observation['facilities']
    p = facilities[geometry.randrange(len(facilities))]
    if geometry.random() < options['other_facility_origin_probability']:
        q = geometry.choice(facilities)
        x, y = q['x'], q['y']
    else:
        axis, delta = geometry.randrange(2), geometry.choice(options['translation_steps'])
        x, y = p['x'] + (delta if axis == 0 else 0), p['y'] + (delta if axis == 1 else 0)
    # Common proposals avoid the old all-catalog-port border restriction. This
    # sampling margin is not a game bound or a new editor feasibility condition.
    x, y = max(1,x), max(1,y)
    original = next(f for f in base['facilities'] if f['instance'] == p['instance'])
    rotation = original['rotation']
    if arm in ('automatic_rotation','automatic_ports','automatic_grid'):
        rotation = extras.choice(base['allowed_rotations'][p['instance']])
    result = dict(kind='place', instance=p['instance'], x=x, y=y, rotation=rotation)
    if arm.startswith('legacy_'):
        result['kind'] = 'pose'
        return result
    result['ports'] = (port_choices(observation, p['instance'], extras)
        if arm in ('automatic_ports','automatic_grid') else assignments(base,p['instance']))
    return result
