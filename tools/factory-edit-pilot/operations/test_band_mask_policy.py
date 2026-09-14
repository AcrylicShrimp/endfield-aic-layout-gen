"""Mask boundary, conditional probability, and native rejection contract checks."""
from copy import deepcopy
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

SOURCE = Path(os.environ.get('AIC_MASK_SOURCE_ROOT', Path(__file__).resolve().parents[3]))
sys.path.insert(0, str(SOURCE / 'tools/factory-edit-pilot'))

import torch
from blueprint_ppo_batch import distribution
from band_mask_policy import BandMask, CODES, EvaluationRuntime, MaskedActor


def observation():
    return dict(blueprint_sha256='example', revision=0, facilities=[dict(x=3, y=5, width=2, height=3)],
        components=[], blueprint=dict(nodes=[dict(terminal=dict(endpoint=dict(kind='external')))]),
        nodes=[dict(position=dict(x=9, y=9))])


def band(kind, at, count=1, axis='x'):
    return dict(kind=kind, at=at, count=count, axis=axis)


class MaskTests(unittest.TestCase):
    def test_body_boundaries_and_external_anchor(self):
        actions = [band('delete_band', x, 2) for x in (1, 2, 3, 4, 5, 8, 9, 10)]
        actions += [band('insert_band', x) for x in (3, 4, 5, 9)]
        mask = BandMask({'components': []})
        self.assertEqual(mask(observation(), actions).tolist(),
            [False, True, True, True, False, True, True, False, False, True, False, False])

    def test_rotated_component_and_facility_port_exemption(self):
        obs = observation(); obs['facilities'] = []
        obs['components'] = [dict(component='fixture', position=dict(x=5, y=5), rotation=90)]
        obs['blueprint']['nodes'][0]['terminal']['endpoint']['kind'] = 'facility'
        mask = BandMask({'components': [dict(id='fixture', footprint=dict(width=2, height=3))]})
        self.assertEqual(mask(obs, [band('delete_band', 7), band('delete_band', 8), band('delete_band', 9),
            band('insert_band', 7), band('insert_band', 8)]).tolist(), [True, False, False, True, False])

    def test_preparatory_placements_remain_and_cache_tracks_revision(self):
        obs = observation(); mask = BandMask({'components': []})
        actions = [dict(kind='place', x=3, y=5), band('delete_band', 3)]
        self.assertEqual(mask(obs, actions).tolist(), [False, True])
        mask(obs, actions); self.assertEqual(mask.cache_hits, 1)
        obs['revision'] += 1; obs['facilities'][0]['x'] = 20
        self.assertEqual(mask(obs, actions).tolist(), [False, False])
        obs['blueprint_sha256'] = 'another'; obs['revision'] = 0; obs['facilities'][0]['x'] = 3
        self.assertEqual(mask(obs, actions).tolist(), [False, True])

    def test_mask_is_conditioning_without_changing_kind_counts(self):
        logits = torch.tensor([1., 2., 3., 4.])
        class Model:
            def root(self, batch): return logits, None, None, None
        runtime = SimpleNamespace(enabled=True, invalid=torch.tensor([False, True, False, False]), mask_apply_seconds=0.)
        batch = dict(size=1, action_graph=torch.zeros(4, dtype=torch.long), kind_log_counts=torch.tensor([0., 1., 1., 1.]))
        original, _ = distribution(logits, batch)
        masked, entropy = distribution(MaskedActor(Model(), runtime).root(batch)[0], batch)
        expected = original.exp(); expected[1] = 0.; expected /= expected.sum()
        torch.testing.assert_close(masked.exp(), expected)
        self.assertEqual(float(masked[1].exp()), 0.)
        self.assertTrue(bool(torch.isfinite(entropy).all()))
        runtime.enabled = False
        self.assertTrue(torch.equal(MaskedActor(Model(), runtime).root(batch)[0], logits))


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'selected release native library required')
class NativeMaskTests(unittest.TestCase):
    def test_candidates_match_native_and_unmasked_sampling_is_identical(self):
        from blueprint_ppo_store import ROOT, read
        from declarative_learning_environment import Runtime
        from declarative_learning_policy import learners, sample
        from test_declarative_learning import diagnostic_config
        torch.set_num_threads(1)
        runtime = Runtime(diagnostic_config(), os.environ['AIC_NATIVE_LIBRARY'])
        session, _ = runtime.open(64, 60000)
        obs = json.loads(session.observation_json())
        checkpoint = json.loads(session.checkpoint_json())
        prepared = runtime.prepare(obs)
        components = read(ROOT / runtime.catalogs['components'])
        mask = BandMask(components)(obs, prepared['actions'])
        native_bands = 0
        # Explicit bounded audit: one native attempt from the same original state
        # for each proposed band; never enters the normal evaluation loop.
        for i, action in enumerate(prepared['actions']):
            if action['kind'] == 'place':
                self.assertFalse(bool(mask[i])); continue
            restored = runtime.restore(checkpoint)
            outcome = runtime.step(restored, obs, action)
            self.assertEqual(bool(mask[i]), bool(CODES.intersection(d['code'] for d in outcome['diagnostics'])), action)
            native_bands += 1
        self.assertGreater(native_bands, 0)
        model, _, _ = learners(17, runtime.config['ppo'])
        instrumented = EvaluationRuntime(runtime, components, False)
        with torch.no_grad():
            a = sample(model, [prepared], torch.Generator().manual_seed(713))
            b = sample(MaskedActor(model, instrumented), [prepared], torch.Generator().manual_seed(713))
        self.assertEqual(a, b)


if __name__ == '__main__':
    unittest.main()
