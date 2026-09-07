import concurrent.futures
import json
from pathlib import Path
import unittest
from unittest.mock import Mock

import torch

from train import advance, validate, Policy, checkpoint, distribution, restore, terminal_advantages


class TrainingTests(unittest.TestCase):
    def test_config_rejects_silent_budget_coercion(self):
        config = json.loads(Path(__file__).with_name("train.json").read_text())
        validate(config)
        for key, value in (("horizon", True), ("workers", 1), ("width", 31), ("episode_ms", 300001),
                           ("entropy", -1), ("evaluation_horizons", [])):
            with self.assertRaises(ValueError):
                validate({**config, key: value})

    def test_done_episodes_are_not_executed(self):
        workers = [Mock(), Mock()]
        workers[1].call.return_value = dict(done=True, marker="advanced")
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
            pool = Mock(workers=workers, executor=executor)
            obs = [dict(done=True), dict(done=False)]
            result = advance(pool, obs, [0, 3])
        workers[0].call.assert_not_called()
        workers[1].call.assert_called_once_with("step", action=3)
        self.assertEqual(result[1]["marker"], "advanced")

    def test_expanded_policy_has_finite_task_gradients_and_exact_restore(self):
        torch.manual_seed(19)
        model = Policy(15, 31, width=8, architecture="graph")
        obs = [dict(features=dict(tokens=[[1.] * 15], adjacency=[[0.]],
                                  actions=[[float(i + j) / 31 for j in range(31)] for i in range(n)],
                                  action_mask=[True] * n)) for n in [2, 4]]
        d = distribution(model, obs)
        signal = terminal_advantages([[100, 50, 9], [101, 1, 0]])
        loss = -(d.log_prob(torch.tensor([0, 3])) * signal).mean()
        loss.backward()
        self.assertTrue(all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()))
        self.assertTrue(any(p.grad is not None and torch.count_nonzero(p.grad) for p in model.parameters()))
        rebuilt = restore(checkpoint(model))
        self.assertTrue(torch.equal(d.logits, distribution(rebuilt, obs).logits))


if __name__ == "__main__":
    unittest.main()
