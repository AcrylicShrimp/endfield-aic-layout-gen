"""Random reset ownership, real native restoration and input provenance regressions."""
from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import torch

from blueprint_ppo import validate
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream, semantic
from blueprint_ppo_store import ROOT, read
from blueprint_ppo_update import digest, learners, update
from blueprint_training_input import FreshInputs, validate_input
from blueprint_evaluation_cohort import case_runtime
from blueprint_ppo_evaluate import evaluate_case


class InputContractTests(unittest.TestCase):
    def test_normal_training_cannot_silently_select_a_fixture(self):
        config = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')
        validate(config)
        config.pop('training_input')
        with self.assertRaisesRegex(ValueError, 'training-input.schema'):
            validate(config)
        config['training_input'] = dict(schema='blueprint-training-input-v1', kind='fixed-fixture-diagnostic')
        with self.assertRaisesRegex(ValueError, 'restricted to explicit diagnostics'):
            validate_input(config)

    def test_episode_slots_never_recycle_seeds_between_streams_or_attempts(self):
        source = FreshInputs.__new__(FreshInputs)
        source.config = dict(training=dict(streams=8))
        source.options = dict(seed_base=123000000, max_attempts=8)
        seeds = set()
        for episode in range(100):
            for stream in range(8):
                row = source.identity(stream, episode)
                current = set(range(row['first_seed'], row['first_seed'] + 8))
                self.assertFalse(seeds & current)
                seeds.update(current)
        self.assertEqual(len(seeds), 6400)
        for stream, episode in [(-1, 0), (8, 0), (0, -1), (True, 0)]:
            with self.assertRaisesRegex(ValueError, 'invalid stream/episode'):
                source.identity(stream, episode)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY') and os.environ.get('AIC_RANDOM_INPUT_TESTS'),
                     'requires native library and explicit real constructor tests')
class RandomNativeTests(unittest.TestCase):
    def test_fresh_episode_inputs_survive_chunk_restore_and_authenticate_witnesses(self):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        config = read(ROOT / 'tools/factory-edit-pilot/blueprint-success-64-12h.json')
        config['training'].update(streams=2, chunk_actions=4, episode_actions=8, updates=4)
        with tempfile.TemporaryDirectory(prefix='blueprint-real-random-') as temporary:
            config['training_input']['directory'] = str(Path(temporary) / 'inputs')
            runtime = Runtime(config, os.environ['AIC_NATIVE_LIBRARY'])
            streams = [Stream(runtime, i) for i in range(2)]
            model, actor, critic = learners(17, config['ppo'])
            sampler, shuffle = torch.Generator().manual_seed(1000017), torch.Generator().manual_seed(2000017)
            first = collect(model, streams, sampler, config, lambda: None)
            inputs = [deepcopy(s.metadata['training_input']) for s in streams]
            self.assertNotEqual(inputs[0]['identity'], inputs[1]['identity'])
            self.assertNotEqual(inputs[0]['request_sha256'], inputs[1]['request_sha256'])
            update(model, actor, critic, shuffle, first['samples'], first['frontiers'], config['ppo'])
            saved = deepcopy(streams[0].snapshot())
            with patch.object(runtime.inputs, 'acquire', side_effect=AssertionError('restoration generated another input')):
                restored = Stream.restore(runtime, saved)
                self.assertEqual(semantic(restored.observe()), first['frontiers'][0]['semantic'])
                self.assertEqual(restored.metadata['training_input'], inputs[0])
                bad = deepcopy(saved)
                bad['metadata']['training_input'] = inputs[1]
                with self.assertRaisesRegex(ValueError, 'wrong episode input record'):
                    Stream.restore(runtime, bad)
                bad = deepcopy(saved)
                bad['metadata'].pop('training_input')
                with self.assertRaisesRegex(ValueError, 'missing episode provenance'):
                    Stream.restore(runtime, bad)
                record = read(Path(temporary) / 'inputs' / inputs[1]['reference']['path'])
                selected = case_runtime(runtime, dict(directory=str(Path(temporary) / 'inputs'), files=record['files']))
                before_inputs = [digest((s.runtime.original, s.runtime.request, s.metadata)) for s in streams]
                before_rng = digest(sampler.get_state())
                evaluate_case(selected, model, 17, 1, dict(name='separate-evaluation', prefix=[]), 0,
                              dict(max_actions=2, elapsed_ms=2000, action_milestones=[2]),
                              Path(temporary) / 'evaluation', lambda: None, 990000001)
                self.assertEqual(before_inputs, [digest((s.runtime.original, s.runtime.request, s.metadata)) for s in streams])
                self.assertEqual(before_rng, digest(sampler.get_state()))
            second = collect(model, streams, sampler, config, lambda: None)
            self.assertEqual(len(second['completed']), 2)
            self.assertTrue(all(e['result']['factory_validated'] for e in second['completed']))
            self.assertEqual(len(list((Path(temporary) / 'inputs').glob('episode-*/episode.json'))), 2)
            for stream in streams:
                stream.observe()
            later = [s.metadata['training_input'] for s in streams]
            self.assertEqual([v['identity']['slot'] for v in later], [2, 3])
            self.assertEqual(len({v['generation_seed'] for v in inputs + later}), 4)
            self.assertGreater(len({v['geometry_sha256'] for v in inputs + later}), 1)
            # A paired policy consumes the exact retained setup; it cannot redraw
            # the same seed and silently substitute another wall-clock outcome.
            other = Stream(runtime, 0)
            with patch.object(runtime.inputs, 'native', None):
                other.observe()
            self.assertEqual(other.metadata['training_input'], inputs[0])
            original = Path(temporary) / 'inputs' / inputs[0]['reference']['path']
            original.write_text(original.read_text() + ' ')
            with self.assertRaisesRegex(ValueError, 'checkpoint/evidence identity'):
                Stream.restore(runtime, saved)


if __name__ == '__main__':
    unittest.main()
