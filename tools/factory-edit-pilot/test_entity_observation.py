"""Semantic identity equivariance and true mixed-count policy/update behavior."""

from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest

import numpy as np
import torch

from native import Runtime
from policy import Policy, PreparedRollout, batch, single, update
from reference.acceptance import Catalog, DIRECTIONS, check
from two_case import validate_compatibility

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


def truncated_chain(original):
    case = deepcopy(original)
    moved = next(iter(next(c for c in case['witness'] if c['op'] == 'SET_PLACEMENTS')['assignments']))
    internal = next(n for n in case['networks'] if n['source'].get('facility') == moved and 'facility' in n['target'])
    downstream = internal['target']['facility']
    kept = {moved, downstream}
    outgoing = next(n for n in case['networks'] if n['source'].get('facility') == downstream and n['target'].get('facility') not in kept)
    case['facilities'] = [f for f in case['facilities'] if f['id'] in kept]
    case['networks'] = [n for n in case['networks'] if n['source'].get('facility') in kept or n['target'].get('facility') in kept]
    case['target'] = {'item': outgoing['item'], 'rate': outgoing['rate']}
    outgoing['target'] = {'external': 'truncated-output'}
    outgoing['corners'] = outgoing['corners'][:1]
    # The seed corridor leaves this producer toward the discarded downstream group.
    source = original['networks'][next(i for i, n in enumerate(original['networks']) if n['id'] == outgoing['id'])]
    a, b = source['corners'][:2]
    direction = ((b[0] > a[0])-(b[0] < a[0]), (b[1] > a[1])-(b[1] < a[1]))
    side = next(k for k, v in DIRECTIONS.items() if v == direction)
    outgoing['target_binding'] = {'cell': outgoing['corners'][0], 'side': side}
    case['id'] = 'two-facility-observation-fixture'
    return case


def renamed_case(original):
    case = deepcopy(original)
    facilities = {f['id']: f'node-{len(case["facilities"])-i}' for i, f in enumerate(case['facilities'])}
    networks = {n['id']: f'edge-{len(case["networks"])-i}' for i, n in enumerate(case['networks'])}
    for f in case['facilities']:
        f['id'] = facilities[f['id']]
    for n in case['networks']:
        n['id'] = networks[n['id']]
        for role in ('source', 'target'):
            if 'facility' in n[role]:
                n[role]['facility'] = facilities[n[role]['facility']]
    case['facilities'].reverse()
    case['networks'].reverse()
    case.pop('witness')
    case['id'] = 'renamed-reordered-observation-fixture'
    return case, facilities, networks


def mapped_command(command, facilities, networks):
    value = deepcopy(command)
    if 'assignments' in value:
        value['assignments'] = {facilities[k]: v for k, v in value['assignments'].items()}
    if 'network' in value:
        value['network'] = networks[value['network']]
    if 'terminal' in value:
        network, role = value['terminal'].rsplit(':', 1)
        value['terminal'] = f'{networks[network]}:{role}'
    return value


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'explicit native build required')
class EntityObservationTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(23)
        self.config = json.loads((DIRECTORY/'run-capacity-width1.json').read_text())
        self.config.update(device='cpu', threads=1, horizon=8, epochs=1, minibatch=4)
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.case = json.loads((DIRECTORY/'case-relocation.json').read_text())
        self.library = Path(os.environ['AIC_NATIVE_LIBRARY'])

    def runtime(self, case, name):
        path = Path(self.temporary.name)/f'{name}.json'
        path.write_text(json.dumps(case))
        return Runtime(ROOT, path, self.config, self.library)

    def test_mixed_entity_counts_padding_prepared_rollouts_and_ppo_update(self):
        small = self.runtime(truncated_chain(self.case), 'small')
        large = self.runtime(self.case, 'large')
        validate_compatibility({'small': small, 'large': large})
        envs = [small.new_engine(), large.new_engine()]
        observations = [env.observe()[0] for env in envs]
        self.assertEqual([len(rt.case['facilities']) for rt in (small, large)], [2, 4])
        self.assertNotEqual(len(observations[0]['entities']), len(observations[1]['entities']))
        model = Policy(observations[0], normalization='batch')
        self.assertEqual(model.parameter_count, Policy(observations[1], normalization='batch').parameter_count)
        model.calibrate_normalization(observations, 2)
        with torch.no_grad():
            together = model(batch(observations))
            for i, observation in enumerate(observations):
                separate = model(single(observation))
                torch.testing.assert_close(together[0][i, :len(observation['candidates'])], separate[0][0], atol=1e-6, rtol=1e-5)
                torch.testing.assert_close(together[1][i], separate[1][0], atol=1e-6, rtol=1e-5)
        rollout = {key: [] for key in ('observations', 'actions', 'log_probs', 'values', 'rewards', 'dones')}
        for i in range(8):
            env = envs[i % 2]
            observation, handle = env.observe()
            action, probability, value = model.act(observation)
            reward, done, _ = env.step(handle, action)
            for key, item in zip(rollout, (observation, action, probability, value, reward, done)):
                rollout[key].append(item)
        advantage = np.zeros(8, dtype=np.float32)
        prepared = PreparedRollout(rollout, advantage, advantage, torch.device('cpu'))
        self.assertEqual(prepared.estimated_bytes, sum(t.numel()*t.element_size() for t in prepared.tensors.values()))
        for indices in (np.array([0, 2]), np.array([1, 3]), np.array([0, 1])):
            actual = prepared.select(indices)
            expected = batch([rollout['observations'][i] for i in indices])
            for key in expected:
                self.assertTrue(torch.equal(actual[key], expected[key]), key)
        before = deepcopy(model.state_dict())
        metrics = update(model, torch.optim.Adam(model.parameters(), lr=self.config['learning_rate']), rollout, self.config, np.random.default_rng(23))
        self.assertTrue(np.isfinite(metrics['policy_loss']))
        self.assertTrue(any(not torch.equal(before[key], value) for key, value in model.state_dict().items()))
        for runtime, env in zip((small, large), envs):
            self.assertTrue(check(Catalog(ROOT, runtime.path), json.loads(env.export_layout_json('best')))['valid'])

    def test_renaming_reordering_and_pair_operand_permutation_preserve_logits(self):
        self.config["horizon"] = 32
        changed, facilities, networks = renamed_case(self.case)
        runtimes = [self.runtime(self.case, 'original'), self.runtime(changed, 'renamed')]
        envs = [runtime.new_engine() for runtime in runtimes]
        model = Policy(envs[0].observe()[0], normalization='batch')
        model.eval()
        for unbound in (False, True):
            if unbound:
                for network in self.case['networks']:
                    for role in ('source', 'target'):
                        command = {'op': 'UNBIND_TERMINALS', 'terminal': f'{network["id"]}:{role}'}
                        for env, current in zip(envs, (command, mapped_command(command, facilities, networks))):
                            env.diagnostic_step_json(json.dumps(current))
            observed = [env.observe() for env in envs]
            outputs = [model(single(o)) for o, _ in observed]
            commands = [json.loads(env.export_candidates_json(handle)) for env, (_, handle) in zip(envs, observed)]
            indexes = {json.dumps(c, sort_keys=True): i for i, c in enumerate(commands[1])}
            remap = [indexes[json.dumps(mapped_command(c, facilities, networks), sort_keys=True)] for c in commands[0]]
            torch.testing.assert_close(outputs[0][0][0], outputs[1][0][0, remap], atol=1e-6, rtol=1e-5)
            torch.testing.assert_close(outputs[0][1], outputs[1][1], atol=1e-6, rtol=1e-5)
            if unbound:
                altered = deepcopy(observed[0][0])
                pairs = [i for i, c in enumerate(commands[0]) if c['op'] == 'SET_PLACEMENTS' and len(c['assignments']) == 2]
                self.assertTrue(pairs)
                for key in ('operands', 'operand_entities', 'operand_mask', 'points'):
                    altered[key][pairs] = altered[key][pairs, ::-1]
                actual = model(single(altered))
                torch.testing.assert_close(outputs[0][0], actual[0], atol=1e-6, rtol=1e-5)
        with self.assertRaisesRegex(ValueError, 'policy.checkpoint_schema'):
            model.load_state_dict({'objects.0.weight': torch.zeros(1)})
