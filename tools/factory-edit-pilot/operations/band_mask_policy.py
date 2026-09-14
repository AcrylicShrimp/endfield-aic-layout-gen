"""Research-only structural band mask; keep the original candidate distribution."""
from collections import Counter
import time

import torch


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


class EvaluationRuntime:
    """Same native evaluator, with episode-local mask and small cost counters."""
    def __init__(self, base, components, enabled):
        self.base, self.enabled = base, enabled
        self.mask = BandMask(components) if enabled else None
        self.invalid = None
        self.diagnostics = Counter()
        self.prepare_seconds = self.mask_seconds = self.mask_apply_seconds = 0.
        self.candidates = self.masked_candidates = 0

    def __getattr__(self, name):
        return getattr(self.base, name)

    def prepare(self, observation):
        began = time.perf_counter()
        prepared = self.base.prepare(observation)
        self.prepare_seconds += time.perf_counter() - began
        if self.enabled:
            began = time.perf_counter()
            self.invalid = self.mask(observation, prepared['actions'])
            self.masked_candidates += int(self.invalid.sum())
            self.mask_seconds += time.perf_counter() - began
        self.candidates += len(prepared['actions'])
        return prepared

    def step(self, session, observation, action):
        outcome = self.base.step(session, observation, action)
        codes = [d['code'] for d in outcome['diagnostics']]
        self.diagnostics.update(codes)
        if self.enabled and CODES.intersection(codes):
            raise ValueError(f'band-mask.unexpected-rejection: {action}: {codes}')
        return outcome

    def metrics(self):
        return dict(enabled=self.enabled, candidates=self.candidates, masked_candidates=self.masked_candidates,
            prepare_seconds=self.prepare_seconds, mask_seconds=self.mask_seconds,
            mask_apply_seconds=self.mask_apply_seconds, cache_hits=self.mask.cache_hits if self.mask else 0,
            diagnostics=dict(self.diagnostics))


class MaskedActor:
    """Evaluation-only view of frozen weights; no optimizer or replay changes."""
    def __init__(self, model, runtime):
        self.model, self.runtime = model, runtime

    def __getattr__(self, name):
        return getattr(self.model, name)

    def root(self, batch):
        logits, value, nodes, pooled = self.model.root(batch)
        if self.runtime.enabled:
            began = time.perf_counter()
            invalid = self.runtime.invalid.to(logits.device)
            if batch['size'] != 1 or invalid.shape != logits.shape or bool(invalid.all()):
                raise ValueError('band-mask.domain: one nonempty evaluation action domain required')
            # A finite sentinel avoids 0 * -inf in the existing entropy kernel.
            logits = logits.masked_fill(invalid, -1e9)
            self.runtime.mask_apply_seconds += time.perf_counter() - began
        return logits, value, nodes, pooled
