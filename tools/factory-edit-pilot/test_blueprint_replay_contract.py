"""Child input namespaces and mixed replay keep source and discovery clocks separate."""
from copy import deepcopy
import os
import tempfile
from pathlib import Path
import unittest
import numpy as np
import torch

from blueprint_replay_buffer import ReplayBuffer
from blueprint_replay_contract import Failure, Sources, classify
from blueprint_training_input import FreshInputs


class ReplayContractTests(unittest.TestCase):
    def test_shared_empty_arrays_can_be_durably_saved_with_distinct_types(self):
        from blueprint_ppo_store import save, load
        backing = bytearray(64)
        values = [torch.from_numpy(np.ndarray((0,), dtype=dtype, buffer=backing))
                  for dtype in (np.float32, np.int64)]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'empty.pt.gz'
            save(path, values)
            restored = load(path)
        self.assertEqual([v.dtype for v in restored], [torch.float32, torch.int64])
        self.assertTrue(all(v.numel() == 0 for v in restored))

    @unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'requires selected native library')
    def test_mixed_wrapper_admits_only_after_true_terminal_and_restores_live_archive(self):
        from test_blueprint_success import SuccessReplayTests, MemoryEvidence
        from blueprint_success_collect import collect_scripted
        SuccessReplayTests.setUpClass()
        fixture = SuccessReplayTests()
        config, streams, model, _, _, sampler, _ = fixture.setup_script(fixture.two_success_script(), 4)
        evidence = MemoryEvidence()
        buffer = ReplayBuffer([], [], dict(large=['fixture'], small=[]))
        for ordinal in (1, 2):
            chunk = collect_scripted(model, streams, sampler, config, lambda: None)
            for before in chunk['native_before']:
                before['metadata']['training_input'] = dict(draw=dict(product='fixture'), identity=dict(fixture=True))
            evidence.chunks[ordinal] = chunk
            buffer.ingest(chunk, ordinal, evidence, config['success'], 1.)
            if ordinal == 1:
                self.assertEqual(buffer.summary(buffer.eligible(evidence, 2))['own_entries'], 0)
                buffer = ReplayBuffer([], [], buffer.strata, buffer.snapshot())
        with self.assertRaisesRegex(ValueError, 'current child experience'):
            buffer.eligible(evidence, 2)
        sequences = buffer.eligible(evidence, 3)
        self.assertEqual(len(sequences), 2)
        self.assertTrue(all(s['origin'] == 'child-own' for s in sequences))
        self.assertAlmostEqual(sequences[0]['rows'][0]['return_to_terminal'] + sequences[1]['rows'][0]['return_to_terminal'], .15)

    def test_child_slots_continue_episode_counters_without_reusing_parent_inputs(self):
        source = FreshInputs.__new__(FreshInputs)
        source.config = dict(training=dict(streams=8))
        source.options = dict(seed_base=391000000, max_attempts=8,
            namespace=dict(schema='blueprint-input-namespace-v1', global_first_slot=2048, slots=512))
        rows = [source.identity(stream, episode) for episode in range(256, 320) for stream in range(8)]
        self.assertEqual([r['slot'] for r in rows], list(range(512)))
        self.assertEqual([r['global_slot'] for r in rows], list(range(2048, 2560)))
        self.assertEqual(rows[0]['first_seed'], 391000000)
        self.assertEqual(rows[-1]['first_seed'], 391004088)
        for episode in (0, 255, 320):
            with self.assertRaisesRegex(ValueError, 'input-namespace'):
                source.identity(0, episode)

    def test_fixed_sources_do_not_increment_child_admissions_and_deduplicate(self):
        parent = dict(id='same', origin='parent-own', stratum='small', child_admitted_update=0,
                      rows=[dict(index=1)], origin_refs=[dict(origin='parent-own', namespace='parent')])
        imported = dict(parent, origin='imported', origin_refs=[dict(origin='imported', namespace='other')])
        pristine = deepcopy(parent)
        buffer = ReplayBuffer([parent], [imported], dict(large=['large-product'], small=['small-product']))
        eligible = buffer.eligible(None, 1)
        self.assertEqual(len(eligible), 1)
        self.assertEqual(len(eligible[0]['origin_refs']), 2)
        self.assertEqual(eligible[0]['origin'], 'parent-own')
        self.assertEqual(buffer.summary(eligible)['own_admissions'], 0)
        self.assertEqual(parent, pristine)
        buffer.mark_used([['same', 'same']])
        restored = ReplayBuffer([parent], [imported], buffer.strata, buffer.snapshot())
        self.assertEqual(restored.summary(restored.eligible(None, 1))['child_selected_uses'], 2)
        self.assertEqual(restored.own.state['admissions'], 0)

    def test_unknown_products_and_escaped_sources_have_structured_failures(self):
        with self.assertRaises(Failure) as raised:
            classify('unexpected', dict(large=['known'], small=[]))
        self.assertEqual(raised.exception.diagnostic['code'], 'replay-balance.source-invalid')
        self.assertEqual(raised.exception.diagnostic['stage'], 'admission')
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(Failure):
                Sources().reference(Path(directory), dict(path='../unowned', sha256='0' * 64))


if __name__ == '__main__':
    unittest.main()
