"""Restart comparison keeps equal command budgets and independently valid incumbents."""

from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest

import torch

from geometry_restart_diagnosis import evaluate_cell, validate_protocol
from native import Runtime
from policy import Policy
from native_training_comparison import policy_hash

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class RestartProtocolTests(unittest.TestCase):
    def test_reject_unequal_budgets_and_overlapping_banks(self):
        spec = json.loads((DIRECTORY/'geometry-restart-diagnosis.json').read_text())
        validate_protocol(spec)
        for key, value in [('attempt_horizons', [128, 30]), ('attempt_horizons', [16, 16]),
                           ('total_commands', 0), ('seed_banks', [700000, 700031]),
                           ('required_successes', 33)]:
            changed = deepcopy(spec)
            changed[key] = value
            with self.assertRaisesRegex(ValueError, 'restart.protocol'):
                validate_protocol(changed)


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'explicit native build required')
class NativeRestartTests(unittest.TestCase):
    def test_equal_command_accounting_valid_retention_and_frozen_bn(self):
        torch.set_num_threads(1)
        torch.manual_seed(41)
        config = json.loads((DIRECTORY/'run-capacity-width1.json').read_text())
        config.update(device='cpu', threads=1, horizon=16)
        runtime = Runtime(ROOT, DIRECTORY/'case-relocation.json', config, Path(os.environ['AIC_NATIVE_LIBRARY']))
        example, _ = runtime.new_engine().observe()
        actor = Policy(example, normalization='batch')
        actor.eval()
        before = policy_hash(actor.state_dict())
        spec = {'episodes': 2, 'total_commands': 32, 'required_successes': 1}
        with tempfile.TemporaryDirectory() as temporary:
            for horizon in (32, 16):
                current = {**config, 'horizon': horizon}
                runtime = Runtime(ROOT, DIRECTORY/'case-relocation.json', current, Path(os.environ['AIC_NATIVE_LIBRARY']))
                directory = Path(temporary)/str(horizon)
                result = evaluate_cell(ROOT, runtime, actor, current, spec, horizon, 900000, directory)
                self.assertEqual(result['charged_commands'], 64)
                self.assertEqual(result['resets_after_initial'], 2*(32//horizon-1))
                self.assertEqual(result['validation']['status'], 'passed')
                self.assertEqual(result['best_validation']['status'], 'passed')
                rows = [json.loads(line) for line in (directory/'evaluations.jsonl').read_text().splitlines()]
                for row in rows:
                    self.assertEqual(row['score'], min(a['score'] for a in row['attempts']))
                    self.assertEqual(sum(a['counts']['commands'] for a in row['attempts']), 32)
        self.assertEqual(before, policy_hash(actor.state_dict()))
