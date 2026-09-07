"""Behavioral checks required before interpreting the first learning result."""

from copy import deepcopy
import json
from pathlib import Path
import random
import tempfile
import unittest

import numpy as np
import torch

from acceptance import Catalog, Invalid, check, port_geometry
from environment import Environment, serialize
from policy import Policy, advantages, batch, update


ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class PilotTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)

    def setUp(self):
        self.catalog = Catalog(ROOT, DIRECTORY/"case.json")
        self.config = json.loads((DIRECTORY/"run.json").read_text())
        self.env = Environment(self.catalog, self.config)

    def execute(self, actions):
        total = 0
        for action in actions:
            self.assertIn(action, self.env.candidates())
            reward, _, result = self.env.step(action)
            self.assertTrue(result["applied"], result)
            total += reward
        return total

    def test_root_witness_complete_and_reward_telescope(self):
        root = check(self.catalog, serialize(self.env.root))
        self.assertTrue(root["valid"])
        reward = self.execute(self.catalog.case["witness"])
        best = check(self.catalog, serialize(self.env.best))
        self.assertTrue(best["valid"])
        self.assertLess(best["score"][0], root["score"][0])
        self.assertAlmostEqual(reward, (root["score"][0]-best["score"][0])/root["score"][0])

    def test_relocation_rerouting_and_valid_base(self):
        actions = [
            {"op": "UNBIND_TERMINALS", "terminal": "glass:source"},
            {"op": "UNBIND_TERMINALS", "terminal": "quartz:target"},
            {"op": "ERASE_TRANSPORT", "network": "glass", "cells": [[1, 13], [1, 12]]},
            {"op": "SET_PLACEMENTS", "assignments": {"F": [0, 12, 0]}},
            {"op": "BIND_TERMINALS", "terminal": "glass:source", "binding": {"port": "output-belt-1"}},
            {"op": "EXTEND", "network": "quartz", "anchor": [1, 17], "growth": "forward", "direction": "N", "length": 2},
            {"op": "BIND_TERMINALS", "terminal": "quartz:target", "binding": {"port": "input-belt-1"}},
        ]
        self.assertEqual(self.execute(actions), 0)
        self.assertTrue(check(self.catalog, serialize(self.env.base))["valid"])
        self.assertNotEqual(self.env.base["placements"], self.env.root["placements"])
        self.assertEqual(self.env.best_score, self.env.initial_score)
        self.execute(self.catalog.case["witness"])
        self.assertTrue(check(self.catalog, serialize(self.env.best))["valid"])
        self.assertLess(self.env.best_score[0], self.env.initial_score[0])

    def test_complete_checker_rejects_semantic_negative_cases(self):
        original = serialize(self.env.root)
        cases = []
        lost = deepcopy(original)
        del lost["placements"]["F"]
        cases.append(lost)
        wrong = deepcopy(original)
        wrong["bindings"]["component:target"] = {"port": "output-belt-1"}
        cases.append(wrong)
        boundary = deepcopy(original)
        boundary["bindings"]["quartz:source"] = {"cell": [1, 18], "side": "S"}
        cases.append(boundary)
        disconnected = deepcopy(original)
        disconnected["networks"]["ore"]["edges"].pop(3)
        cases.append(disconnected)
        collision = deepcopy(original)
        collision["placements"]["F"] = [0, 0, 0]
        cases.append(collision)
        for case in cases:
            result = check(self.catalog, case)
            self.assertFalse(result["valid"], result)
            self.assertIsNone(result["score"])

    def test_request_capacity_and_coverage_checked_before_learning(self):
        for field, rate in (("rate", [3, 4]), ("rate", [1, 8])):
            case = deepcopy(self.catalog.case)
            case["networks"][0][field] = rate
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp)/"case.json"
                path.write_text(json.dumps(case))
                with self.assertRaises(Invalid):
                    Catalog(ROOT, path)

    def test_rejected_and_stale_commands_do_not_mutate(self):
        initial = serialize(self.env.draft)
        _, _, outcome = self.env.step({"op": "SET_PLACEMENTS", "assignments": {"F": [0, 0, 0]}})
        self.assertFalse(outcome["applied"])
        self.assertEqual(serialize(self.env.draft), initial)
        self.assertEqual(self.env.steps, 1)
        _, _, stale = self.env.step({"op": "UNBIND_TERMINALS", "terminal": "quartz:source", "revision": 0})
        self.assertFalse(stale["applied"])
        self.assertEqual(serialize(self.env.draft), initial)

    def test_discard_and_reset_have_different_bases(self):
        self.execute(self.catalog.case["witness"])
        improved = serialize(self.env.base)
        self.env.step({"op": "UNBIND_TERMINALS", "terminal": "quartz:source"})
        self.env.step({"op": "DISCARD_EDIT"})
        self.assertEqual(serialize(self.env.draft), improved)
        self.assertEqual(self.env.steps, len(self.catalog.case["witness"])+2)
        self.env.reset()
        self.assertEqual(serialize(self.env.draft), serialize(self.env.root))
        self.assertEqual(self.env.best_score, self.env.initial_score)
        self.assertEqual(self.env.steps, 0)

    def test_restoring_base_is_not_a_new_completion(self):
        tid = "battery:target"
        binding = deepcopy(self.env.draft["bindings"][tid])
        self.env.step({"op": "UNBIND_TERMINALS", "terminal": tid})
        _, _, outcome = self.env.step({"op": "BIND_TERMINALS", "terminal": tid, "binding": binding})
        self.assertTrue(outcome["complete"])
        self.assertFalse(outcome["new_completion"])
        self.env.step({"op": "UNBIND_TERMINALS", "terminal": tid})
        _, _, outcome = self.env.step({"op": "DISCARD_EDIT"})
        self.assertTrue(outcome["complete"])
        self.assertFalse(outcome["new_completion"])

    def test_candidate_replay_and_geometry_encodings(self):
        rng = random.Random(99)
        for _ in range(2):
            self.env.reset()
            for step in range(96):
                candidates = self.env.candidates()
                observation = self.env.observe(candidates)
                self.assertTrue(np.isfinite(observation["raster"]).all())
                self.assertTrue(np.isfinite(observation["objects"]).all())
                command = candidates[rng.randrange(len(candidates))]
                _, _, outcome = self.env.step(command)
                self.assertTrue(outcome["applied"], (command, outcome))

    def test_port_transforms_agree_with_independent_checker(self):
        for tid in self.catalog.terminal_ids:
            _, _, endpoint = self.catalog.terminal(tid)
            if "facility" not in endpoint:
                continue
            fid = endpoint["facility"]
            binding = self.env.draft["bindings"][tid]
            for rotation in self.catalog.facility_type(fid)["allowed_rotations"]:
                state = deepcopy(self.env.draft)
                state["placements"][fid] = [7, -4, rotation]
                expected = port_geometry(self.catalog, fid, state["placements"][fid], binding["port"])
                self.assertEqual(self.env.location(state, tid), expected[:2])

    def test_padding_preserves_log_probability(self):
        first = self.env.observe(self.env.candidates())
        self.execute([{"op": "UNBIND_TERMINALS", "terminal": "quartz:source"},
                      {"op": "EXTEND", "network": "quartz", "anchor": [1, 19], "growth": "backward", "direction": "W", "length": 2}])
        second = self.env.observe(self.env.candidates())
        model = Policy(first)
        with torch.no_grad():
            solo, solo_value = model(batch([first]))
            together, values = model(batch([first, second]))
        torch.testing.assert_close(solo[0], together[0, :solo.shape[1]], atol=1e-6, rtol=1e-5)
        torch.testing.assert_close(solo_value[0], values[0], atol=1e-6, rtol=1e-5)
        self.assertTrue(all(p.device.type == "cpu" for p in model.parameters()))

    def test_ppo_finite_update_and_finite_horizon_credit(self):
        adv, returns = advantages([0, 1, 0, 0], [0, 0, 0, 0], [False, True, False, True], 99, 1, 1)
        np.testing.assert_array_equal(returns, [1, 1, 0, 0])
        config = {**self.config, "horizon": 8, "epochs": 1, "minibatch": 8}
        env = Environment(self.catalog, config)
        model = Policy(env.observe(env.candidates()))
        optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
        before = {k: v.clone() for k, v in model.state_dict().items()}
        rollout = {k: [] for k in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
        for _ in range(16):
            candidates = env.candidates()
            observation = env.observe(candidates)
            action, log_prob, value = model.act(observation)
            reward, done, _ = env.step(candidates[action])
            for key, item in (("observations", observation), ("actions", action), ("log_probs", log_prob),
                              ("values", value), ("rewards", reward), ("dones", done)):
                rollout[key].append(item)
            if done:
                env.reset()
        result = update(model, optimizer, rollout, config, np.random.default_rng(7))
        self.assertTrue(all(np.isfinite(v) for v in result.values()))
        self.assertGreater(result["gradient_norm"], 0)
        self.assertTrue(any(not torch.equal(v, before[k]) for k, v in model.state_dict().items()))


if __name__ == "__main__":
    unittest.main()
