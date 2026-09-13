"""Pure successful-segment geometry shared by admission and offline diagnostics."""
from collections import Counter
import math

FIELDS = ('instance', 'x', 'y', 'width', 'height', 'rotation')


def required(config):
    return ((config.get('retention') or {}).get('schema') == 'declarative-replay-category-v2' or
            (config.get('sampling') or {}).get('kind') == 'score_mixture')


def validate_metadata(value, identity):
    require(isinstance(value, dict) and set(value) == {'has_place', 'gain', 'exterior_release'},
            'retention-metadata', identity)
    require(type(value['has_place']) is bool and
            all(type(value[k]) in (int, float) and math.isfinite(value[k]) for k in ('gain', 'exterior_release')) and
            0 < value['gain'] < 1 and 0 <= value['exterior_release'] <= 1,
            'retention-metadata', identity)


def segment_row(samples, identity='admission'):
    """Convert all changed labels, including actual post-place geometry."""
    last = samples[-1]
    witness = last['validated_witness']
    require(last['reward'] > 0 and witness is not None, 'not-success', identity)
    layouts = [[[p[k] for k in FIELDS] for p in s['observation']['facilities']] for s in samples]
    final = [[p[k] for k in FIELDS] for p in witness['placements']]
    aliases = {p[0]: i for i, p in enumerate(layouts[0])}
    def geometry(layout):
        require(set(p[0] for p in layout) == set(aliases), 'identity', identity)
        return sorted([[aliases[p[0]], *p[1:]] for p in layout])
    actions, places = [], []
    for index, sample in enumerate(samples):
        action = dict(sample['action'])
        if action['kind'] == 'place':
            action['instance'] = aliases[action['instance']]
            after = layouts[index + 1] if index + 1 < len(samples) else final
            places.append(dict(label=index, instance=action['instance'],
                before=geometry(layouts[index]), after=geometry(after)))
        actions.append(dict(step=sample['policy_action'], action=action))
    best = samples[0]['observation']['best_metrics']
    after_extent = witness_extent(witness)
    return dict(id=identity, stratum=None, product=None, geometry=None, episode=None,
        depth=None, admitted_update=None, uses=None, actions=actions, places=places,
        before_area=best['score'][0], after_area=math.prod(after_extent),
        before_extent=[best['width'], best['height']], after_extent=after_extent,
        before=geometry(layouts[0]), final=geometry(final), native_validated=True)


def extract(samples, identity):
    value = features(segment_row(samples, identity))
    result = {k:value[k] for k in ('has_place', 'gain', 'exterior_release')}
    validate_metadata(result, identity)
    return result

def require(ok, code, message):
    if not ok:
        raise ValueError(f"success-scoring.{code}: {message}")


def bounds(layout):
    return (min(r[1] for r in layout), min(r[2] for r in layout),
            max(r[1] + r[3] for r in layout), max(r[2] + r[4] for r in layout))


def extent(layout):
    x0, y0, x1, y1 = bounds(layout)
    return x1 - x0, y1 - y0


def witness_extent(value):
    # Validation first canonicalizes the origin using ALL referenced geometry;
    # metrics then measures facility bodies and network cells in that frame.
    # A raw validated_draft witness has not necessarily been canonicalized.
    x0, y0, x1, y1 = bounds([[p[k] for k in FIELDS] for p in value['placements']])
    for network in value['transport_networks']:
        for cell in network['cells']:
            x0, y0 = min(x0, cell['x']), min(y0, cell['y'])
            x1, y1 = max(x1, cell['x'] + 1), max(y1, cell['y'] + 1)
        points = [t['position'] for t in network.get('terminals', [])]
        points += [s[k] for s in network.get('segments', []) for k in ('from', 'to')]
        for point in points:
            x0, y0 = min(x0, point['x']), min(y0, point['y'])
    for component in value.get('logistics_components', []):
        x0, y0 = min(x0, component['position']['x']), min(y0, component['position']['y'])
    return x1 - x0, y1 - y0


def validate_row(row):
    require(row['native_validated'] is True, 'not-validated', row['id'])
    require(0 < row['after_area'] < row['before_area'], 'area', row['id'])
    require(math.prod(row['before_extent']) == row['before_area'] and
            math.prod(row['after_extent']) == row['after_area'], 'extent', row['id'])
    require(bool(row['actions']), 'actions', row['id'])
    for layout in [row['before'], row['final']] + [p[k] for p in row['places'] for k in ('before', 'after')]:
        require(bool(layout) and len({p[0] for p in layout}) == len(layout), 'geometry', row['id'])
        require(all(len(p) == 6 and all(isinstance(v, int) for v in p) and
            p[1] >= 0 and p[2] >= 0 and p[3] > 0 and p[4] > 0 for p in layout), 'geometry', row['id'])
    expected = [(i, a['action']['instance']) for i, a in enumerate(row['actions']) if a['action']['kind'] == 'place']
    require(expected == [(p['label'], p['instance']) for p in row['places']], 'place-coverage', row['id'])
    require(set(p[0] for p in row['before']) == set(p[0] for p in row['final']), 'identity', row['id'])


def features(row):
    validate_row(row)
    kinds = [a['action']['kind'] for a in row['actions']]
    require(set(kinds) <= {'place', 'insert_band', 'delete_band'}, 'kind', row['id'])
    family = ('place_final' if kinds[-1] == 'place' else 'place_preparation' if 'place' in kinds
              else 'insert_preparation' if 'insert_band' in kinds else 'delete_only')
    x0, y0, x1, y1 = bounds(row['before'])
    center = [(x0 + x1) / 2, (y0 + y1) / 2]
    diagonal = math.hypot(*row['before_extent'])
    inward = Counter()
    extent_release = 0.
    for place in row['places']:
        before = {p[0]:p for p in place['before']}
        after = {p[0]:p for p in place['after']}
        owner = place['instance']
        require(set(before) == set(after) and owner in before, 'identity', row['id'])
        require(all(before[k] == after[k] for k in before if k != owner), 'place-transition', row['id'])
        a, b = before[owner], after[owner]
        def distance(p):
            return math.hypot(p[1] + p[3]/2 - center[0], p[2] + p[4]/2 - center[1])
        # Signed changes cancel an inward move followed by its reversal. Max
        # over facilities avoids a bonus merely for adding more edited objects.
        inward[owner] += (distance(a) - distance(b)) / diagonal
        extent_release += (math.prod(extent(place['before'])) - math.prod(extent(place['after']))) / row['before_area']
    return dict(id=row['id'], stratum=row['stratum'], product=row['product'], geometry=row['geometry'],
        episode=row['episode'], depth=row['depth'], family=family, has_place=bool(row['places']),
        labels=len(kinds), gain=(row['before_area'] - row['after_area']) / row['before_area'],
        center_inward=min(1., max([0., *inward.values()])),
        exterior_release=min(1., max(0., extent_release)),
        admitted_update=row['admitted_update'], uses=row['uses'])


def score(row, weights):
    center, exterior = row['center_inward'], row['exterior_release']
    if weights.get('cap_geometry', False):
        center, exterior = min(center, row['gain']), min(exterior, row['gain'])
    return (row['gain'] + weights['rare'] * row['rarity'] +
            weights['center'] * center + weights['exterior'] * exterior)
