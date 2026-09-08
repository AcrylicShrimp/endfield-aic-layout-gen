"""Behavioral checks required before interpreting the first learning result."""

from copy import deepcopy
from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import random
import sys
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
import torch

from reference.acceptance import Catalog, Invalid, check, port_geometry
from reference.environment import Environment, serialize
from reference.commands import Command
from policy import Policy, advantages, batch, single, update, PreparedRollout
from run import refresh_actor
from native import Runtime
import run as runner


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
            self.assertIn(action, self.env.candidates().to_dicts())
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
                self.assertTrue(np.isfinite(observation["entities"]).all())
                command = candidates.commands[rng.randrange(len(candidates.commands))].to_dict()
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
            reward, done, _ = env.step(candidates.commands[action].to_dict())
            for key, item in (("observations", observation), ("actions", action), ("log_probs", log_prob),
                              ("values", value), ("rewards", reward), ("dones", done)):
                rollout[key].append(item)
            if done:
                env.reset()
        result = update(model, optimizer, rollout, config, np.random.default_rng(7))
        self.assertTrue(all(np.isfinite(v) for k, v in result.items() if k not in ("seconds", "preparation", "normalization_calibration")))
        self.assertTrue(all(np.isfinite(v) and v >= 0 for v in result["seconds"].values()))
        self.assertGreater(result["gradient_norm"], 0)
        self.assertTrue(any(not torch.equal(v, before[k]) for k, v in model.state_dict().items()))

    @unittest.skipUnless(torch.cuda.is_available(), "CUDA device required")
    def test_cuda_outputs_and_gradients_agree_on_padded_inputs(self):
        torch.backends.cuda.matmul.allow_tf32 = False
        torch.backends.cudnn.allow_tf32 = False
        first = self.env.observe(self.env.candidates())
        self.execute([{"op": "UNBIND_TERMINALS", "terminal": "quartz:source"},
                      {"op": "EXTEND", "network": "quartz", "anchor": [1, 19], "growth": "backward", "direction": "W", "length": 2}])
        data = batch([first, self.env.observe(self.env.candidates())])
        cpu = Policy(first)
        cuda = deepcopy(cpu).to("cuda")
        outputs = []
        for model in (cpu, cuda):
            logits, values = model({key: value.to(next(model.parameters()).device) for key, value in data.items()})
            log_probs = logits.log_softmax(-1)
            (log_probs[:, 0].mean()+values.square().mean()).backward()
            outputs.append((log_probs.detach().cpu(), values.detach().cpu()))
        for a, b in zip(*outputs):
            torch.testing.assert_close(a, b, atol=2e-6, rtol=2e-4)
        for a, b in zip(cpu.parameters(), cuda.parameters()):
            torch.testing.assert_close(a.grad, b.grad.cpu(), atol=2e-6, rtol=2e-4)

    @unittest.skipUnless(torch.cuda.is_available(), "CUDA device required")
    def test_cuda_ppo_refreshes_cpu_actor_after_update(self):
        config = {**self.config, "horizon": 8, "epochs": 1, "minibatch": 8}
        env = Environment(self.catalog, config)
        actor = Policy(env.observe(env.candidates()))
        learner = deepcopy(actor).to("cuda")
        optimizer = torch.optim.Adam(learner.parameters(), lr=config["learning_rate"])
        before = deepcopy(actor.state_dict())
        rollout = {k: [] for k in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
        for _ in range(16):
            candidates = env.candidates()
            observation = env.observe(candidates)
            action, log_prob, value = actor.act(observation)
            reward, done, _ = env.step(candidates.commands[action].to_dict())
            for key, value in (("observations", observation), ("actions", action), ("log_probs", log_prob),
                               ("values", value), ("rewards", reward), ("dones", done)):
                rollout[key].append(value)
            if done:
                env.reset()
        metrics = update(learner, optimizer, rollout, config, np.random.default_rng(7))
        self.assertTrue(all(np.isfinite(v) for k, v in metrics.items() if k not in ("seconds", "preparation", "normalization_calibration")))
        self.assertGreater(metrics["gradient_norm"], 0)
        self.assertTrue(all(torch.equal(value, before[key]) for key, value in actor.state_dict().items()))
        refresh_actor(learner, actor)
        self.assertTrue(any(not torch.equal(value, before[key]) for key, value in actor.state_dict().items()))
        self.assertTrue(all(torch.equal(value, learner.state_dict()[key].cpu()) for key, value in actor.state_dict().items()))
        self.assertTrue(all(p.device.type == "cpu" for p in actor.parameters()))
        actor.act(rollout["observations"][0], torch.Generator().manual_seed(19))

    def test_prepared_rollout_preserves_minibatch_shapes_values_and_budget(self):
        observations = [self.env.observe(self.env.candidates())]
        self.execute([{"op": "UNBIND_TERMINALS", "terminal": "quartz:source"},
                      {"op": "EXTEND", "network": "quartz", "anchor": [1, 19], "growth": "backward", "direction": "W", "length": 2}])
        observations.append(self.env.observe(self.env.candidates()))
        rollout = {"observations": observations, "actions": [0, 1], "log_probs": [-1.0, -2.0]}
        advantage = np.array([.25, -.25], dtype=np.float32)
        returns = np.array([.5, 0], dtype=np.float32)
        prepared = PreparedRollout(rollout, advantage, returns, torch.device("cpu"))
        for indices in (np.array([0]), np.array([1]), np.array([1, 0])):
            selected = prepared.select(indices)
            expected = batch([observations[i] for i in indices])
            for key, value in expected.items():
                self.assertEqual(selected[key].shape, value.shape)
                self.assertEqual(selected[key].dtype, value.dtype)
                self.assertTrue(torch.equal(selected[key], value))
            self.assertTrue(torch.equal(selected["advantage"], torch.from_numpy(advantage[indices])))
            self.assertTrue(torch.equal(selected["returns"], torch.from_numpy(returns[indices])))
        self.assertEqual(sum(v.numel()*v.element_size() for v in prepared.tensors.values()), prepared.estimated_bytes)
        limited = PreparedRollout(rollout, advantage, returns, torch.device("cpu"), maximum_bytes=1)
        self.assertIsNone(limited.tensors)
        exact = PreparedRollout(rollout, advantage, returns, torch.device("cpu"),
                                maximum_bytes=prepared.estimated_bytes)
        self.assertIsNotNone(exact.tensors)
        for budget in (-1, True, 1.5):
            with self.assertRaisesRegex(ValueError, "preparation.invalid_budget"):
                PreparedRollout(rollout, advantage, returns, torch.device("cpu"), maximum_bytes=budget)

    def test_invalid_preparation_budget_reports_configuration_path(self):
        with tempfile.TemporaryDirectory() as directory:
            for budget in (-1, True, 1.5):
                config = {**self.config, "prepared_rollout_max_bytes": budget}
                with self.assertRaisesRegex(ValueError, "preparation.invalid_budget: bad-config.json"):
                    runner.train(self.catalog, config, Path("bad-config.json"), Path(directory)/"run")

    def test_canonical_command_cache_eviction_preserves_json_order(self):
        first = Command("EXTEND", (self.env.nids[0], (0, 0), "forward", "N", 1))
        expected = json.dumps(first.to_dict(), sort_keys=True)
        self.assertEqual(self.env.canonical_command_key(first), expected)
        for x in range(1, 4200):
            command = Command("EXTEND", (self.env.nids[0], (x, 0), "forward", "N", 1))
            self.assertEqual(self.env.canonical_command_key(command), json.dumps(command.to_dict(), sort_keys=True))
        self.assertEqual(self.env.canonical_command_key.cache_info().currsize, 4096)
        self.assertEqual(self.env.canonical_command_key(first), expected)
        candidates = self.env.candidates().to_dicts()
        self.assertEqual(candidates, sorted(candidates, key=lambda c: json.dumps(c, sort_keys=True)))

    def test_candidate_encoding_eviction_preserves_observation(self):
        candidates = self.env.candidates()
        before = self.env.observe(candidates)
        first = Command("EXTEND", (self.env.nids[0], (0, 0), "forward", "N", 1))
        original, *operands = self.env.encode_candidate(first, None)
        self.assertFalse(original.flags.writeable)
        for x in range(1, 4200):
            command = Command("EXTEND", (self.env.nids[0], (x, 0), "forward", "N", 1))
            self.env.encode_candidate(command, None)
        self.assertEqual(self.env.encode_candidate.cache_info().currsize, 4096)
        np.testing.assert_array_equal(original, self.env.encode_candidate(first, None)[0])
        after = self.env.observe(candidates)
        for key in before:
            np.testing.assert_array_equal(before[key], after[key])

    def test_candidate_snapshot_rejects_observation_after_state_change(self):
        candidates = self.env.candidates()
        self.env.step({"op": "UNBIND_TERMINALS", "terminal": "quartz:source"})
        with self.assertRaisesRegex(ValueError, "observation.stale_candidates"):
            self.env.observe(candidates)
        self.env.reset()
        with self.assertRaisesRegex(ValueError, "observation.stale_candidates"):
            self.env.observe(candidates)

    def test_single_observation_view_matches_padded_batch_without_mutation(self):
        observation = self.env.observe(self.env.candidates())
        before = deepcopy(observation)
        model = Policy(observation)
        view, padded = single(observation), batch([observation])
        for key in padded:
            self.assertTrue(torch.equal(view[key], padded[key]))
        with torch.no_grad():
            for actual, expected in zip(model(view), model(padded)):
                torch.testing.assert_close(actual, expected, atol=0, rtol=0)
        model.act(observation, torch.Generator().manual_seed(7))
        for key in before:
            np.testing.assert_array_equal(before[key], observation[key])

    def test_observation_categories_do_not_alias_later_observations(self):
        candidates = self.env.candidates()
        first = self.env.observe(candidates)
        expected = deepcopy(first)
        for key, value in first.items():
            if key != "schema_version":
                value.fill(-1)
        second = self.env.observe(candidates)
        for key in expected:
            np.testing.assert_array_equal(second[key], expected[key])
        self.env.step({"op": "UNBIND_TERMINALS", "terminal": "quartz:source"})
        self.env.observe(self.env.candidates())
        self.env.reset()
        third = self.env.observe(self.env.candidates())
        for key in expected:
            np.testing.assert_array_equal(third[key], expected[key])

    @unittest.skipUnless(sys.platform == "linux", "Compiled CPU actor is deployed on Linux")
    def test_compiled_actor_dynamic_shapes_refresh_and_rng(self):
        from compiled_actor import CompiledActor
        first = self.env.observe(self.env.candidates())
        self.execute([{"op": "UNBIND_TERMINALS", "terminal": "quartz:source"},
                      {"op": "EXTEND", "network": "quartz", "anchor": [1, 19], "growth": "backward", "direction": "W", "length": 2}])
        second = self.env.observe(self.env.candidates())
        actor = Policy(first)
        before = deepcopy([first, second])
        callbacks = len(torch._dynamo.callback_handler.start_callbacks)
        with tempfile.TemporaryDirectory(dir=ROOT/"target") as directory:
            compiled = CompiledActor(actor, directory)
            try:
                rng = torch.get_rng_state().clone()
                with torch.no_grad():
                    for observation in (first, second):
                        for actual, expected in zip(compiled(single(observation)), actor(single(observation))):
                            torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
                self.assertTrue(torch.equal(rng, torch.get_rng_state()))
                events = len(compiled.events)
                self.assertGreater(events, 0)
                learner = deepcopy(actor)
                with torch.no_grad():
                    for parameter in learner.parameters():
                        parameter.add_(0.001)
                refresh_actor(learner, actor)
                with torch.no_grad():
                    for observation in (first, second):
                        for actual, expected in zip(compiled(single(observation)), actor(single(observation))):
                            torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
                self.assertEqual(len(compiled.events), events)
                selected, log_prob, value = actor.act(second, torch.Generator().manual_seed(7), forward=compiled)
                self.assertLess(selected, len(second["candidates"]))
                self.assertTrue(np.isfinite([log_prob, value]).all())
            finally:
                compiled.close()
        self.assertEqual(len(torch._dynamo.callback_handler.start_callbacks), callbacks)
        for observation, expected in zip((first, second), before):
            for key in observation:
                np.testing.assert_array_equal(observation[key], expected[key])

    @unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build path required")
    def test_completed_cpu_continuation_matches_uninterrupted_training(self):
        config = {**self.config, "device": "cpu", "threads": 1,
                  "horizon": 8, "commands": 32, "rollout_commands": 16, "epochs": 1,
                  "minibatch": 8, "checkpoints": [0, 16, 32], "evaluation_seeds": 2}
        (ROOT/"target").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT/"target") as temporary:
            root = Path(temporary)
            def run(name, settings, parent=None):
                output = root/name
                output.mkdir()
                path = output/"config.json"
                path.write_text(json.dumps(settings))
                with patch("torch.set_num_interop_threads"), patch("plots.render"), redirect_stdout(io.StringIO()):
                    runtime = Runtime(ROOT, DIRECTORY/"case.json", settings, os.environ["AIC_NATIVE_LIBRARY"])
                    runner.train(runtime, settings, path, output, parent)
                return output
            full = run("full", config)
            first = run("first", {**config, "commands": 16, "checkpoints": [0, 16]})
            parent = first/"checkpoint-16.pt"
            continuation = {**config, "checkpoints": [16, 32]}
            second = run("second", continuation, parent)
            a = torch.load(full/"checkpoint-32.pt", weights_only=False)
            b = torch.load(second/"checkpoint-32.pt", weights_only=False)
            def equal(a, b):
                if isinstance(a, torch.Tensor):
                    self.assertTrue(torch.equal(a, b))
                elif isinstance(a, np.ndarray):
                    np.testing.assert_array_equal(a, b)
                elif isinstance(a, dict):
                    self.assertEqual(a.keys(), b.keys())
                    for key in a:
                        equal(a[key], b[key])
                elif isinstance(a, (tuple, list)):
                    self.assertEqual(len(a), len(b))
                    for x, y in zip(a, b):
                        equal(x, y)
                else:
                    self.assertEqual(a, b)
            for key in ("policy", "optimizer", "torch_rng", "numpy_rng", "numpy_legacy_rng", "python_rng",
                        "commands", "episode", "optimizer_steps", "counts", "champion_score"):
                equal(a[key], b[key])
            self.assertEqual((full/"episodes.jsonl").read_text(),
                             (first/"episodes.jsonl").read_text()+(second/"episodes.jsonl").read_text())
            metadata = json.loads((second/"metadata.json").read_text())
            self.assertEqual(metadata["resumed_commands"], 16)
            self.assertEqual(metadata["segment_commands"], 16)
            with self.assertRaisesRegex(ValueError, "configuration changed"):
                run("invalid-config", {**continuation, "learning_rate": .001}, parent)
            with self.assertRaisesRegex(ValueError, "final checkpoint"):
                run("invalid-checkpoint", continuation, first/"checkpoint-0.pt")
            original = json.loads((first/"metadata.json").read_text())
            for module in ("native.py", "run.py"):
                altered = deepcopy(original)
                altered["provenance"]["files_sha256"][f"tools/factory-edit-pilot/{module}"] = "changed"
                (first/"metadata.json").write_text(json.dumps(altered))
                with self.assertRaisesRegex(ValueError, "provenance disagree"):
                    run(f"invalid-{module}", continuation, parent)
            (first/"metadata.json").write_text(json.dumps(original))
            saved = torch.load(parent, weights_only=False)
            legacy = deepcopy(saved)
            legacy["schema_version"] = 1
            legacy_path = first/"legacy.pt"
            torch.save(legacy, legacy_path)
            with self.assertRaisesRegex(ValueError, "resume.legacy_checkpoint"):
                run("invalid-legacy", continuation, legacy_path)
            changed = deepcopy(saved)
            changed["environment"]["best_score"][0] = 1
            changed_path = first/"invalid-native-state.pt"
            torch.save(changed, changed_path)
            with self.assertRaisesRegex(ValueError, "snapshot.score"):
                run("invalid-native-state", continuation, changed_path)

    @unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build path required")
    def test_interrupted_collection_restores_partial_native_episode(self):
        config = {**self.config, "device": "cpu", "threads": 1,
                  "horizon": 8, "commands": 16, "rollout_commands": 16, "epochs": 1,
                  "minibatch": 8, "checkpoints": [0, 16], "evaluation_seeds": 2}
        original_act = Policy.act
        calls = 0
        def interrupt(policy, *args, **kwargs):
            nonlocal calls
            calls += 1
            if calls == 20:  # Sixteen frozen-evaluation actions, then three training edits.
                raise KeyboardInterrupt
            return original_act(policy, *args, **kwargs)
        (ROOT/"target").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT/"target") as directory:
            output = Path(directory)
            path = output/"config.json"
            path.write_text(json.dumps(config))
            runtime = Runtime(ROOT, DIRECTORY/"case.json", config, os.environ["AIC_NATIVE_LIBRARY"])
            with patch.object(Policy, "act", interrupt), patch("torch.set_num_interop_threads"), \
                    patch("plots.render"), redirect_stdout(io.StringIO()):
                with self.assertRaises(KeyboardInterrupt):
                    runner.train(runtime, config, path, output)
            saved = torch.load(output/"interrupted-state.pt", weights_only=False)
            self.assertEqual(saved["phase"], "collection")
            self.assertEqual(saved["commands"], 3)
            self.assertEqual(saved["environment"]["steps"], 3)
            self.assertEqual(len(saved["partial_rollout"]["actions"]), 3)
            restored = runtime.new_engine()
            restored.restore_state_json(json.dumps(saved["environment"]))
            reference = Environment(self.catalog, config)
            for event in saved["environment"]["trace"]:
                reference.step(event["command"])
            candidates = reference.candidates()
            expected = reference.observe(candidates)
            actual, handle = restored.observe()
            for key in expected:
                np.testing.assert_array_equal(actual[key], expected[key])
            model = Policy(actual)
            model.load_state_dict(saved["actor_policy"])
            torch.set_rng_state(saved["torch_rng"])
            selected, _, _ = model.act(actual)
            self.assertEqual(restored.step(handle, selected), reference.step(candidates.commands[selected].to_dict()))

    @unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build path required")
    def test_runner_preserves_interruption_diagnostics(self):
        config = {**self.config, "device": "cpu", "threads": 2,
                  "horizon": 8, "commands": 16, "rollout_commands": 16, "epochs": 1,
                  "minibatch": 8, "checkpoints": [0, 16], "evaluation_seeds": 2}
        (ROOT/"target").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT/"target") as temporary:
            output = Path(temporary)
            config_path = output/"config.json"
            config_path.write_text(json.dumps(config))
            with patch.object(runner, "update", side_effect=KeyboardInterrupt), \
                    patch("torch.set_num_interop_threads"), patch("plots.render"), redirect_stdout(io.StringIO()):
                with self.assertRaises(KeyboardInterrupt):
                    runtime = Runtime(ROOT, DIRECTORY/"case.json", config, os.environ["AIC_NATIVE_LIBRARY"])
                    runner.train(runtime, config, config_path, output)
            metadata = json.loads((output/"metadata.json").read_text())
            self.assertEqual(metadata["status"], "interrupted")
            self.assertEqual(metadata["completed_commands"], 16)
            self.assertEqual(metadata["error"]["message"], "KeyboardInterrupt")
            self.assertGreater(metadata["inference_seconds"], 0)
            self.assertGreater(metadata["environment_seconds"]["observation"], 0)
            retained = torch.load(output/"interrupted-state.pt", weights_only=False, map_location="cpu")
            self.assertEqual(len(retained["partial_rollout"]["observations"]), 16)
            self.assertEqual(retained["schema_version"], 2)
            self.assertEqual(retained["environment"]["schema_version"], 2)
            restored = runtime.new_engine()
            restored.restore_state_json(json.dumps(retained["environment"]))
            self.assertEqual(json.loads(restored.export_state_json())["draft"], retained["environment"]["draft"])
        torch.set_num_threads(1)


if __name__ == "__main__":
    unittest.main()
