import json
from pathlib import Path
import tempfile
import unittest

import torch

from client import Pool
from policy import DirectPolicy, decode, checkpoint, restore, location_mask
from train import terminal_advantages, validate, credit_signals


class DirectPolicyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(2)
        cls.temp = tempfile.TemporaryDirectory(prefix="aic-direct-test-")
        cls.pool = Pool(Path(__file__).with_name("sampling.json").resolve(), Path(cls.temp.name), 1)
        cls.root = cls.pool.workers[0].call("generate", seed="streaming-train-01", ordinal=22, split="train")["root"]
        cls.obs = cls.pool.workers[0].call("reset", root=cls.root, budget=dict(steps=16, elapsed_ms=300000))

    @classmethod
    def tearDownClass(cls):
        cls.pool.close()
        cls.temp.cleanup()

    def test_no_candidate_list_and_exact_direct_replay_with_task_gradient(self):
        self.assertNotIn("actions", self.obs["state"])
        self.assertNotIn("action_mask", self.obs["state"])
        torch.manual_seed(123)
        model = DirectPolicy(16)
        action, choices, logp, _ = decode(model, self.obs)
        replay = decode(model, self.obs, decisions=choices)
        self.assertEqual(action, replay[0])
        self.assertTrue(torch.equal(logp, replay[2]))
        (-logp).backward()
        self.assertTrue(all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()))
        self.assertTrue(any(p.grad is not None and torch.count_nonzero(p.grad) for p in model.parameters()))
        rebuilt = restore(checkpoint(model))
        self.assertTrue(torch.equal(replay[2], decode(rebuilt, self.obs, decisions=choices)[2]))

    def test_location_mask_matches_exhaustive_body_collision_without_ranked_pose_filtering(self):
        state = self.obs["state"]
        for member in range(len(state["facilities"])):
            p = state["facilities"][member]
            location = location_mask(state, [member])
            mask, b = location["legal"], state["bounds"]
            for y in range(mask.shape[0]):
                for x in range(mask.shape[1]):
                    px, py = x + b["minX"], y + b["minY"]
                    blocked = any(i != member and px < q["x"] + q["width"] and q["x"] < px + p["width"]
                                  and py < q["y"] + q["height"] and q["y"] < py + p["height"] for i, q in enumerate(state["facilities"]))
                    self.assertEqual(bool(mask[y, x]), not blocked)

    def test_direct_decisions_execute_and_keep_an_audited_incumbent(self):
        worker = self.pool.workers[0]
        obs = worker.call("reset", root=self.root, budget=dict(steps=8, elapsed_ms=300000))
        generator = torch.Generator().manual_seed(193)
        for _ in range(8):
            action, _, _, _ = decode(None, obs, generator=generator, uniform=True)
            obs = worker.call("step", action=action)
            self.assertLessEqual(tuple(obs["best_score"]), tuple(obs["initial_score"]))
        self.assertTrue(obs["done"])
        self.assertTrue(worker.call("audit")["coverage"]["factory_validated"])

    def test_terminal_priority_and_configuration(self):
        self.assertEqual(terminal_advantages([[100, 99, 5], [101, 0, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 5], [100, 9, 5]]), [0, 0])
        validate(json.loads(Path(__file__).with_name("train.json").read_text()))
        validate(json.loads(Path(__file__).with_name("credit.json").read_text()))
        validate(json.loads(Path(__file__).with_name("breadth.json").read_text()))
        with self.assertRaises(ValueError):
            validate(dict(json.loads(Path(__file__).with_name("breadth.json").read_text()), training_size_schedule={"small": 1}))
        with self.assertRaises(ValueError):
            validate(dict(schema_version=1))

    def test_futility_tiebreak_cannot_outvote_any_quality_field(self):
        self.assertEqual(terminal_advantages([[100, 99, 5, 999], [101, 0, 0, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 99, 999], [100, 10, 0, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 5, 999], [100, 9, 6, 0]]), [1, -1])
        self.assertEqual(terminal_advantages([[100, 9, 5, 1], [100, 9, 5, 2]]), [1, -1])

    def test_temporal_baseline_uses_only_pre_action_history(self):
        def entry(score, futile=0):
            return (dict(best_score=score, futile=futile), [], {})
        histories = [[entry([100, 9, 5]), entry([90, 9, 5])], [entry([100, 9, 5]), entry([100, 9, 5])]]
        summaries = [dict(best_score=dict(tuple=q), waste=dict(futile=0)) for q in ([90, 9, 5], [100, 9, 5])]
        credit = credit_signals(histories, summaries, "quality-futility-prefix")
        self.assertEqual(credit["signals"], [[1, 0], [-1, 0]])
        self.assertEqual(credit_signals(histories, summaries, "terminal-score")["signals"], [[1, 1], [-1, -1]])
        # Delayed success is credited before its later improvement; no valid-worse penalty.
        histories[0][1] = entry([100, 9, 5])
        self.assertEqual(credit_signals(histories, summaries, "quality-futility-prefix")["signals"], [[1, 1], [-1, -1]])
        with self.assertRaises(ValueError):
            credit_signals([histories[0], []], summaries, "quality-futility-prefix")


if __name__ == "__main__":
    unittest.main()
