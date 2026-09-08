"""Mastery thresholds, isolated assessments and multi-round PPO accumulation."""

from contextlib import redirect_stdout
from copy import deepcopy
from dataclasses import asdict
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
import torch

from mastery import Mastery, assess
from mastery_study import load, train
from native_training_comparison import equal, policy_hash
from policy import Policy, advantages, update

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class MasteryControllerTests(unittest.TestCase):
    def test_both_cases_two_consecutive_checks_and_restore(self):
        protocol = json.loads((DIRECTORY / "mastery-study.json").read_text())
        controller = Mastery()
        self.assertEqual(controller.observe({"A": 64, "B": 51}, protocol)["after"], {"level": 1, "streak": 0})
        controller.observe({"A": 52, "B": 52}, protocol)
        controller = Mastery(**asdict(controller))
        self.assertEqual(controller.observe({"A": 52, "B": 52}, protocol)["after"], {"level": 2, "streak": 0})
        controller.observe({"A": 64, "B": 64}, protocol)
        self.assertEqual(controller.observe({"A": 0, "B": 64}, protocol)["after"], {"level": 2, "streak": 0})
        for _ in range(4):
            controller.observe({"A": 64, "B": 64}, protocol)
        self.assertEqual(asdict(controller), {"level": 0, "streak": 0})
        with self.assertRaisesRegex(ValueError, "mastery.state"):
            controller.observe({"A": 65, "B": 64}, protocol)


@unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build required")
class MasteryIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.library = Path(os.environ["AIC_NATIVE_LIBRARY"]).resolve()
        config = json.loads((DIRECTORY / "run-server-cuda.json").read_text())
        config.update(device="cpu", threads=1, horizon=32, evaluation_seeds=2, epochs=1, minibatch=16)
        (self.directory / "config.json").write_text(json.dumps(config))
        protocol = json.loads((DIRECTORY / "mastery-study.json").read_text())
        protocol.update(base_config=str(self.directory / "config.json"), commands=256, streams=2, rollout_commands=64,
                        training_seeds=[17], checkpoints=[0, 64, 128, 192, 256],
                        fixed_curriculum=[{"until": 64, "remaining": 1}, {"until": 128, "remaining": 2}, {"until": 192, "remaining": 3}])
        protocol["assessment"].update(interval=64, episodes=4, required_successes=3)
        for spec in protocol["conditions"].values():
            if spec["horizon"] == 128:
                spec["horizon"] = 32
        self.protocol_path = self.directory / "protocol.json"
        self.protocol_path.write_text(json.dumps(protocol))
        torch.set_num_threads(1)

    def test_assessment_preserves_policy_training_rng_and_training_horizon(self):
        protocol, _, evaluation, training, groups = load(ROOT, self.protocol_path, "short-mastery", 17, self.library)
        self.assertEqual((evaluation["horizon"], training["horizon"]), (32, 8))
        actor = Policy(groups[0]["A"].new_engine().observe()[0])
        original = policy_hash(actor.state_dict())
        before = torch.get_rng_state().clone()
        result = assess(actor, groups[2], protocol, 1)
        torch.testing.assert_close(torch.get_rng_state(), before, rtol=0, atol=0)
        self.assertEqual(original, policy_hash(actor.state_dict()))
        self.assertEqual(len(result["rows"]), 8)
        self.assertTrue(all(row["counts"]["commands"] == 8 for row in result["rows"]))

    def test_short_rounds_have_fixed_update_size_and_complete_gae_boundaries(self):
        received = []
        def checked(model, optimizer, rollout, config, rng):
            self.assertEqual(len(rollout["actions"]), 64)
            self.assertEqual([i for i, d in enumerate(rollout["dones"]) if d], list(range(7,64,8)))
            actual = advantages(rollout["rewards"], rollout["values"], rollout["dones"],0,1,.95)[0]
            expected = np.concatenate([advantages(rollout["rewards"][i:i+8], rollout["values"][i:i+8], rollout["dones"][i:i+8],0,1,.95)[0]
                                       for i in range(0,64,8)])
            np.testing.assert_array_equal(actual, expected)
            received.append(len(actual))
            return update(model, optimizer, rollout, config, rng)
        with redirect_stdout(io.StringIO()), patch("mastery_study.update", side_effect=checked):
            result = train(ROOT, self.protocol_path, "short-fixed", 17, self.library, self.directory / "run")
        self.assertEqual(received, [64]*4)
        self.assertEqual(result["episodes"], 32)
        self.assertEqual(result["case_commands"], {"A":128,"B":128})
        self.assertEqual(result["setup_commands"], 60)

    def test_interruption_keeps_accumulated_rounds_and_current_partial_round(self):
        from curriculum import Collector
        original = Collector.collect
        calls = []
        def interrupted(collector, model, remaining):
            calls.append(1)
            if len(calls) == 3:
                raise RuntimeError("controlled collection interruption")
            return original(collector, model, remaining)
        output = self.directory / "interrupted"
        with redirect_stdout(io.StringIO()), patch.object(Collector, "collect", interrupted):
            with self.assertRaisesRegex(RuntimeError,"controlled collection interruption"):
                train(ROOT, self.protocol_path, "short-mastery", 17, self.library, output)
        saved = torch.load(output / "interrupted-state.pt", map_location="cpu", weights_only=False)
        self.assertEqual(saved["metadata"]["completed_commands"], 32)
        self.assertEqual(len(saved["partial_rollout"]["actions"]), 32)
        self.assertEqual(len(saved["stream_rngs"]), 2)
        self.assertEqual(saved["metadata"]["controller"], {"level":1,"streak":0})


if __name__ == "__main__":
    unittest.main()
