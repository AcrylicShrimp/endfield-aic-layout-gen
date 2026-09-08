"""Batched evaluation retains seeded episode boundaries and independently replayable actions."""

from copy import deepcopy
import json
import os
from pathlib import Path
import unittest

import torch

from case_curriculum import assess_cases
from case_curriculum_audit import replay_assessment
from curriculum import act_batch, prefixes
from evaluation_batch import batch_size, episodes
from native import Runtime, layout, trace
from native_training_comparison import policy_hash
from policy import Policy
from reference.acceptance import Catalog, check

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class EvaluationBudgetTests(unittest.TestCase):
    def test_batch_width_is_explicit_and_positive(self):
        self.assertEqual(batch_size({}), 16)
        for value in (0, -1, True, 1.5):
            with self.assertRaisesRegex(ValueError, 'evaluation.batch_size'):
                batch_size({'evaluation_batch_size': value})


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'explicit native build required')
class BatchedEvaluationTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(19)
        self.config = json.loads((DIRECTORY/'run-capacity-width1.json').read_text())
        self.config.update(device='cpu', threads=1, horizon=16, evaluation_batch_size=2)
        self.runtime = Runtime(ROOT, DIRECTORY/'case-relocation.json', self.config, Path(os.environ['AIC_NATIVE_LIBRARY']))
        self.actor = Policy(self.runtime.new_engine().observe()[0], normalization='batch')
        self.catalog = Catalog(ROOT, self.runtime.path)

    def test_partial_final_wave_keeps_all_episode_seeds_and_valid_roots(self):
        before = policy_hash(self.actor.state_dict())
        rng = torch.get_rng_state().clone()
        result = list(episodes(self.actor, self.runtime, 5, 16, 920000, 2))
        self.assertEqual([seed for seed, _, _ in result], list(range(920000, 920005)))
        for seed, env, total in result:
            self.assertEqual(len(trace(env)), 16)
            accepted = check(self.catalog, layout(env, 'best'))
            self.assertTrue(accepted['valid'])
            self.assertAlmostEqual(total, (env.initial_score[0]-accepted['score'][0])/env.initial_score[0])
        self.assertEqual(policy_hash(self.actor.state_dict()), before)
        torch.testing.assert_close(torch.get_rng_state(), rng)

    def test_seeded_traces_match_across_wave_widths(self):
        torch.manual_seed(97)
        actor = Policy(self.runtime.new_engine().observe()[0], normalization='batch')
        expected = [trace(env) for _, env, _ in episodes(actor, self.runtime, 5, 16, 810000, 1)]
        for size in (2, 3, 5):
            actual = [trace(env) for _, env, _ in episodes(actor, self.runtime, 5, 16, 810000, size)]
            self.assertEqual(actual, expected)

    def test_collection_sampling_ignores_companion_candidate_padding(self):
        observation = self.runtime.new_engine().observe()[0]
        companion = deepcopy(observation)
        # The companion increases padded candidate capacity without changing this row.
        for key in ('candidates', 'operands', 'operand_entities', 'operand_mask', 'points'):
            companion[key] = companion[key].repeat(3, axis=0)
        alone = torch.Generator().manual_seed(0)
        together = torch.Generator().manual_seed(0)
        other = torch.Generator().manual_seed(1)
        for _ in range(16):
            a = act_batch(self.actor, [observation], [alone])[0]
            b = act_batch(self.actor, [observation, companion], [together, other])[0]
            self.assertEqual(a[0], b[0])
            self.assertAlmostEqual(a[1], b[1], places=5)
            self.assertAlmostEqual(a[2], b[2], places=5)
            self.assertTrue(torch.equal(alone.get_state(), together.get_state()))

    def test_assessment_commands_replay_without_resampling_and_reject_tampering(self):
        protocol = {'assessment': {'episodes': 5, 'horizon': 16, 'seed_start': 930000}}
        starts = prefixes(self.runtime)
        for level in (1, max(starts)):
            result = assess_cases(self.actor, {'A': self.runtime}, protocol, {'A': level}, self.config)
            self.assertEqual(result['schema_version'], 2)
            self.assertEqual(result['policy_sha256'], policy_hash(self.actor.state_dict()))
            for row in result['rows']:
                self.assertEqual(replay_assessment(self.catalog, self.config, starts[level], row, 16), row)
            changed = deepcopy(result['rows'][0])
            changed['actions'].pop()
            with self.assertRaisesRegex(ValueError, 'command count'):
                replay_assessment(self.catalog, self.config, starts[level], changed, 16)
            changed = deepcopy(result['rows'][0])
            changed['actions'][0] = {'op': 'NOT_AN_EDIT'}
            with self.assertRaisesRegex(ValueError, 'outside the domain'):
                replay_assessment(self.catalog, self.config, starts[level], changed, 16)
