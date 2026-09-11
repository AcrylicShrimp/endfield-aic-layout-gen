"""Behavior checks for the new graph input and independent topology audit."""
from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest

import torch

from blueprint_edit_audit import audit
from blueprint_edit_input import BlueprintActorCritic, prepare
from native import load_native
from probe_blueprint_edit import ROOT, model_smoke, request_from_initial


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires explicitly selected native library')
class BlueprintContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.spec = json.loads((ROOT / 'tools/factory-edit-pilot/blueprint-edit-probe.json').read_text())
        cls.original = json.loads((ROOT / cls.spec['initial']).read_text())
        cls.request = request_from_initial(cls.original)
        cls.module = load_native(Path(os.environ['AIC_NATIVE_LIBRARY']))
        torch.set_num_threads(1)

    def session(self):
        return self.module.BlueprintEditSession.open(str(ROOT), json.dumps(self.spec['catalogs']), json.dumps(self.request),
            json.dumps(self.original), json.dumps(self.spec['config']))

    def test_overlap_keeps_entities_and_model_consumes_blueprint(self):
        session = self.session()
        initial = json.loads(session.observation_json())
        outcome = json.loads(session.step_json(json.dumps(dict(blueprint_sha256=initial['blueprint_sha256'], revision=0,
            action=dict(kind='pose', instance='A0', x=2, y=7, rotation=90)))))
        self.assertEqual(outcome['status'], 'draft_accepted')
        after = json.loads(session.observation_json())
        batch = prepare(after)
        self.assertEqual(len(batch['node_static']), len(after['facilities']) + len(after['blueprint']['nodes']))
        torch.testing.assert_close(batch['node_state'][0, :2], batch['node_state'][1, :2])
        self.assertGreater(batch['node_state'][0, 6].item(), 0.)
        self.assertGreater(batch['node_state'][1, 6].item(), 0.)
        self.assertTrue(model_smoke(after)['original_blueprint_changes_predictions'])
        self.assertEqual(initial['blueprint'], after['blueprint'])

    def test_audit_rejects_rewiring_that_preserves_the_old_request(self):
        session = self.session()
        blueprint = json.loads(session.observation_json())['blueprint']
        changed = deepcopy(self.original)
        poses = {p['instance']: p for p in self.original['placements']}
        swap = {'A0': 'A1', 'A1': 'A0'}
        for placement in changed['placements']:
            if placement['instance'] in swap:
                other = poses[swap[placement['instance']]]
                placement['x'], placement['y'] = other['x'], other['y']
        for network in changed['transport_networks']:
            for terminal in network['terminals']:
                endpoint = terminal['endpoint']
                if endpoint['kind'] == 'facility' and endpoint['instance'] in swap:
                    endpoint['instance'] = swap[endpoint['instance']]
                    terminal['node'] = swap[terminal['node']]
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            (directory / 'request.json').write_text(json.dumps(self.request))
            (directory / 'changed.json').write_text(json.dumps(changed))
            old = self.module.FactoryEditSession.open(str(ROOT), str(directory / 'request.json'), str(directory / 'changed.json'),
                json.dumps(self.spec['catalogs']), json.dumps(dict(proposals=1, elapsed_ms=1000)))
            self.assertTrue(json.loads(old.snapshot_json())['factory_validated'])
        with self.assertRaisesRegex(ValueError, 'terminal .* changed'):
            audit(self.original, blueprint, changed)
        # Native actions also reject an attempted target change before mutation.
        observation = json.loads(session.observation_json())
        outcome = json.loads(session.step_json(json.dumps(dict(blueprint_sha256=observation['blueprint_sha256'], revision=0,
            action=dict(kind='route', edge=0, cells=observation['edges'][0]['path'], target=2)))))
        self.assertEqual(outcome['diagnostics'][0]['code'], 'blueprint.action-schema')
        self.assertEqual(json.loads(session.observation_json())['revision'], 0)

    def test_route_geometry_and_best_record_affect_model_inputs(self):
        observation = json.loads(self.session().observation_json())
        torch.manual_seed(29)
        model = BlueprintActorCritic()
        initial_logits, initial_value = model(prepare(observation))
        changed = deepcopy(observation)
        changed['edges'][0]['path'][1]['y'] += 1
        logits, value = model(prepare(changed))
        self.assertGreater((logits - initial_logits).abs().max().item(), 1e-8)
        self.assertGreater((value - initial_value).abs().item(), 1e-8)
        changed = deepcopy(observation)
        changed['best_metrics']['score'][0] *= 0.9
        logits, value = model(prepare(changed))
        self.assertGreater((logits - initial_logits).abs().max().item(), 1e-8)
        self.assertGreater((value - initial_value).abs().item(), 1e-8)


if __name__ == '__main__':
    unittest.main()
