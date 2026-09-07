import copy
from pathlib import Path
import tempfile
import unittest

import torch

from client import Pool
from gradient_audit import arm, cosine, decomposition, raw_gradient, vector
from policy import DirectPolicy, decode
from replay import snapshot, restore_snapshot
from update import apply_update, behavior


class GradientAuditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(2)
        cls.temp = tempfile.TemporaryDirectory(prefix="aic-gradient-test-")
        cls.pool = Pool(Path(__file__).with_name("sampling.json").resolve(), Path(cls.temp.name), 1)
        root = cls.pool.workers[0].call("generate", seed="streaming-train-01", ordinal=22, split="train")["root"]
        cls.obs = cls.pool.workers[0].call("reset", root=root, budget=dict(steps=16, elapsed_ms=300000))

    @classmethod
    def tearDownClass(cls):
        cls.pool.close()
        cls.temp.cleanup()

    def fixture(self, model):
        histories = []
        for seed in range(4):
            trace = []
            with torch.no_grad():
                action, decisions, logp, entropy = decode(model, self.obs, generator=torch.Generator().manual_seed(seed), trace=trace)
            obs = dict(self.obs, behavior=behavior(logp, entropy, trace))
            histories.append([(obs, decisions, action)])
        return histories

    def test_components_match_independent_autograd_and_reject_stale_behavior(self):
        torch.manual_seed(91)
        model = DirectPolicy(16)
        histories = self.fixture(model)
        signals = [[1.0], [-1.0], [.5], [-.5]]
        task, _ = raw_gradient(model, histories, signals, 0)
        entropy, _ = raw_gradient(model, histories, [[0.0]] * 4, .0001)
        combined, _ = raw_gradient(model, histories, signals, .0001)
        self.assertLess(decomposition(combined, task, entropy)["relative_residual"], 1e-5)
        # Independent joint expression, not the diagnostic streaming helper.
        model.zero_grad(set_to_none=True)
        loss = 0
        for history, credit in zip(histories, signals):
            obs, decisions, _ = history[0]
            _, _, logp, ent = decode(model, obs, decisions=decisions)
            loss = loss + (-credit[0] * logp - .0001 * ent) / 4
        loss.backward()
        torch.testing.assert_close(vector(model, gradient=True), combined, rtol=1e-5, atol=1e-8)
        bad = copy.deepcopy(histories)
        bad[0][0][0]["behavior"]["log_probability"] += .1
        with self.assertRaisesRegex(ValueError, "Behavior replay"):
            raw_gradient(model, bad, signals, 0)
        with self.assertRaises(ValueError):
            raw_gradient(model, histories, [], 0)

    def test_zero_current_signal_separates_adam_momentum(self):
        torch.manual_seed(92)
        model = DirectPolicy(16)
        optimizer = torch.optim.Adam(model.parameters(), lr=.003)
        histories = self.fixture(model)
        zero = [[0.0]] * 4
        initial = snapshot(model, optimizer)
        report, gradient, _ = arm(initial, histories, zero, 0)
        self.assertEqual(report["parameter_delta_norm"], 0)
        self.assertEqual(float(gradient.norm()), 0)
        apply_update(model, optimizer, histories, [[1.0], [-1.0], [.5], [-.5]], 0)
        histories = self.fixture(model)
        before = snapshot(model, optimizer)
        carried, _, _ = arm(before, histories, zero, 0)
        fresh, _, _ = arm(before, histories, zero, 0, clear_moments=True)
        self.assertGreater(carried["parameter_delta_norm"], 0)
        self.assertEqual(fresh["parameter_delta_norm"], 0)
        # Analysis mutates only restored copies.
        self.assertEqual(snapshot(model, optimizer), before)
        restored, restored_optimizer = restore_snapshot(before)
        self.assertEqual(snapshot(restored, restored_optimizer), before)

    def test_undefined_cosines_and_failed_decomposition(self):
        zero, one = torch.zeros(2), torch.ones(2)
        self.assertIsNone(cosine(zero, one))
        self.assertAlmostEqual(cosine(one, -one), -1)
        self.assertIsNone(decomposition(zero, zero, zero)["entropy_to_task_norm"])
        with self.assertRaises(ValueError):
            decomposition(one, zero, zero)


if __name__ == "__main__":
    unittest.main()
