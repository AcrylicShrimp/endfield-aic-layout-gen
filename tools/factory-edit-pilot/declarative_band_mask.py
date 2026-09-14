"""Explicit structural band feasibility masks shared by training and research."""
import torch

MODE = 'structural-bands-v1'
ROOT_MASK_SCHEMA = 'declarative-root-mask-v1'


def validate_mode(actions):
    if actions.get('masking') not in (None, MODE):
        raise ValueError('declarative.masking.config: expected structural-bands-v1 or no masking')


CODES = frozenset(('declarative.band-cuts-body', 'declarative.band-cuts-component',
    'declarative.band-removes-anchor'))


class BandMask:
    def __init__(self, components):
        self.components = {c['id']: c['footprint'] for c in components['components']}
        self.key, self.blocked = None, None
        self.cache_hits = 0

    def __call__(self, observation, actions):
        counts = tuple(sorted({a['count'] for a in actions if a['kind'] == 'delete_band'}))
        key = observation['blueprint_sha256'], observation['revision'], counts
        if key != self.key:
            blocked = {(kind, axis, count): set() for axis in ('x', 'y')
                for kind, widths in (('insert_band', (None,)), ('delete_band', counts)) for count in widths}
            bodies = [(f['x'], f['y'], f['width'], f['height']) for f in observation['facilities']]
            for c in observation['components']:
                footprint = self.components[c['component']]
                w, h = footprint['width'], footprint['height']
                if c['rotation'] % 180:
                    w, h = h, w
                bodies.append((c['position']['x'], c['position']['y'], w, h))
            for x, y, w, h in bodies:
                for axis, origin, size in (('x', x, w), ('y', y, h)):
                    blocked['insert_band', axis, None].update(range(origin + 1, origin + size))
                    for count in counts:
                        blocked['delete_band', axis, count].update(range(origin - count + 1, origin + size))
            for node, current in zip(observation['blueprint']['nodes'], observation['nodes'], strict=True):
                terminal = node['terminal']
                if terminal is not None and terminal['endpoint']['kind'] == 'facility':
                    continue
                for axis in ('x', 'y'):
                    point = current['position'][axis]
                    for count in counts:
                        blocked['delete_band', axis, count].update(range(point - count + 1, point + 1))
            self.key, self.blocked = key, blocked
        else:
            self.cache_hits += 1
        # Keep every candidate and its index; count normalization stays identical.
        return torch.tensor([False if a['kind'] == 'place' else
            a['at'] in self.blocked[a['kind'], a['axis'], a['count'] if a['kind'] == 'delete_band' else None]
            for a in actions], dtype=torch.bool)


