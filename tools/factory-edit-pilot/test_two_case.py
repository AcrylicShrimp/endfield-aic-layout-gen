"""Two-case scheduling, witness parity, evaluation isolation and interruption evidence."""

from contextlib import redirect_stdout
from copy import deepcopy
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np
import torch

from native import Runtime, layout
from native_training_comparison import equal, policy_hash
from policy import Policy, advantages, update
from reference.acceptance import Catalog
from reference.environment import Environment, serialize
from two_case import evaluate_cases, load_study, train, validate_compatibility

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


@unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build path required")
class TwoCaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.library = Path(os.environ["AIC_NATIVE_LIBRARY"]).resolve()
        self.config = json.loads((DIRECTORY / "run-server-cuda.json").read_text())
        self.config.update(device="cpu", threads=1, horizon=32, rollout_commands=64,
                           epochs=1, minibatch=32, evaluation_seeds=2)
        (self.directory / "config.json").write_text(json.dumps(self.config))
        self.protocol = json.loads((DIRECTORY / "two-case-study.json").read_text())
        self.protocol.update(base_config=str(self.directory / "config.json"), checkpoints=[0, 64], training_seeds=[17])
        for condition in self.protocol["conditions"].values():
            condition["commands"] = 64
        self.protocol_path = self.directory / "protocol.json"
        self.protocol_path.write_text(json.dumps(self.protocol))
        torch.set_num_threads(1)

    def load(self, condition="A+B"):
        return load_study(ROOT, self.protocol_path, condition, 17, self.library)

    def assert_observation_equal(self, actual, expected):
        self.assertEqual(actual.keys(), expected.keys())
        for key in actual:
            self.assertEqual(actual[key].dtype, expected[key].dtype, key)
            self.assertEqual(actual[key].shape, expected[key].shape, key)
            self.assertEqual(actual[key].tobytes(), expected[key].tobytes(), key)

    def test_spread_witness_has_five_complete_gains_and_native_reference_parity(self):
        _, _, config, runtimes = self.load()
        runtime = runtimes["B"]
        native = runtime.new_engine()
        reference = Environment(Catalog(ROOT, runtime.path), config)
        self.assertEqual(native.initial_score, [385, 50, 2])
        areas = [native.initial_score[0]]
        for command in runtime.case["witness"]:
            observed, handle = native.observe()
            expected = reference.candidates()
            candidates = json.loads(native.export_candidates_json(handle))
            self.assertEqual(candidates, expected.to_dicts())
            self.assert_observation_equal(observed, reference.observe(expected))
            result = native.step(handle, candidates.index(command))
            self.assertEqual(result, reference.step(command))
            for field in ("draft", "base", "best"):
                self.assertEqual(layout(native, field), serialize(getattr(reference, field)))
            if result[0] > 0:
                self.assertTrue(result[2]["complete"])
                areas.append(native.best_score[0])
        self.assertEqual(areas, [385, 363, 341, 319, 297, 275])
        self.assertEqual(native.best_score, [275, 36, 2])
        generator = np.random.default_rng(93)
        native.reset()
        reference.reset()
        for _ in range(96):
            observed, handle = native.observe()
            expected = reference.candidates()
            candidates = json.loads(native.export_candidates_json(handle))
            self.assertEqual(candidates, expected.to_dicts())
            self.assert_observation_equal(observed, reference.observe(expected))
            selected = int(generator.integers(len(candidates)))
            result = native.step(handle, selected)
            self.assertEqual(result, reference.step(candidates[selected]))
            if result[1]:
                native.reset()
                reference.reset()

    def test_category_reordering_and_incomplete_cycles_are_rejected(self):
        _, _, config, runtimes = self.load()
        changed = deepcopy(runtimes["B"].case)
        changed["networks"].reverse()
        path = self.directory / "reordered.json"
        path.write_text(json.dumps(changed))
        incompatible = Runtime(ROOT, path, config, self.library)
        with self.assertRaisesRegex(ValueError, "study.production"):
            validate_compatibility({"A": runtimes["A"], "B": incompatible})
        self.protocol["conditions"]["A+B"]["commands"] = 96
        self.protocol["checkpoints"] = [0, 96]
        self.protocol_path.write_text(json.dumps(self.protocol))
        with self.assertRaisesRegex(ValueError, "study.allowance"):
            self.load()

    def test_mixed_rollout_accounting_and_terminal_masks(self):
        captured = []
        def observed_update(model, optimizer, rollout, config, rng):
            captured.append(deepcopy(rollout))
            return update(model, optimizer, rollout, config, rng)
        output = self.directory / "mixed"
        with patch("two_case.update", side_effect=observed_update), patch("plots.render"), redirect_stdout(io.StringIO()):
            result = train(ROOT, self.protocol_path, "A+B", 17, self.library, output)
        self.assertEqual(result["case_commands"], {"A": 32, "B": 32})
        self.assertEqual(result["case_episodes"], {"A": 1, "B": 1})
        self.assertEqual(result["optimizer_steps"], 2)
        self.assertEqual(len(captured), 1)
        self.assertEqual(np.flatnonzero(captured[0]["dones"]).tolist(), [31, 63])
        values, _ = advantages([0.] * 32 + [1.] * 32, [0.] * 64, captured[0]["dones"], 0., 1., 1.)
        np.testing.assert_array_equal(values[:32], np.zeros(32))
        self.assertEqual(values[32], 32.)
        rows = [json.loads(line) for line in (output / "episodes.jsonl").read_text().splitlines()]
        self.assertEqual([row["case"] for row in rows], ["A", "B"])
        saved = torch.load(output / "checkpoint-64.pt", weights_only=False, map_location="cpu")
        self.assertEqual(saved["next_schedule_index"], 0)
        self.assertEqual({key: value["steps"] for key, value in saved["environments"].items()}, {"A": 0, "B": 0})
        with self.assertRaisesRegex(ValueError, "study.output_exists"):
            train(ROOT, self.protocol_path, "A+B", 17, self.library, output)

    def test_evaluation_preserves_model_and_global_sampling_rng(self):
        _, _, config, runtimes = self.load()
        observed, _ = runtimes["A"].new_engine().observe()
        model = Policy(observed)
        before_model = policy_hash(model.state_dict())
        before_torch, before_numpy = torch.get_rng_state().clone(), deepcopy(np.random.get_state())
        output = self.directory / "evaluation"
        for key in runtimes:
            (output / key).mkdir(parents=True)
        with redirect_stdout(io.StringIO()):
            result = evaluate_cases(model, runtimes, config, output, 0, {"A": 0, "B": 0}, 0.)
        self.assertEqual(set(result), {"A", "B"})
        self.assertEqual(policy_hash(model.state_dict()), before_model)
        self.assertTrue(torch.equal(torch.get_rng_state(), before_torch))
        self.assertTrue(equal(np.random.get_state(), before_numpy))
        self.assertTrue(model.training)

    def test_partial_second_case_is_retained_and_replays_independently(self):
        original_act = Policy.act
        calls = 0
        def interrupt(actor, observation, generator=None, forward=None):
            nonlocal calls
            if generator is None:
                calls += 1
                if calls == 36:
                    raise KeyboardInterrupt("stop inside the B episode")
            return original_act(actor, observation, generator, forward)
        output = self.directory / "interrupted"
        with patch.object(Policy, "act", interrupt), redirect_stdout(io.StringIO()), self.assertRaises(KeyboardInterrupt):
            train(ROOT, self.protocol_path, "A+B", 17, self.library, output)
        saved = torch.load(output / "interrupted-state.pt", weights_only=False, map_location="cpu")
        self.assertEqual(saved["commands"], 35)
        self.assertEqual(saved["case_commands"], {"A": 32, "B": 3})
        self.assertEqual(saved["case_episodes"], {"A": 1, "B": 0})
        self.assertEqual(saved["active_case"], "B")
        self.assertEqual(saved["next_schedule_index"], 1)
        self.assertEqual(len(saved["partial_rollout"]["actions"]), 35)
        self.assertEqual(saved["environments"]["A"]["steps"], 0)
        self.assertEqual(saved["environments"]["B"]["steps"], 3)
        _, _, config, runtimes = self.load()
        native = runtimes["B"].new_engine()
        native.restore_state_json(json.dumps(saved["environments"]["B"]))
        reference = Environment(Catalog(ROOT, runtimes["B"].path), config)
        for row in saved["environments"]["B"]["trace"]:
            reference.step(row["command"])
        observed, _ = native.observe()
        self.assert_observation_equal(observed, reference.observe(reference.candidates()))
        for field in ("draft", "base", "best"):
            self.assertEqual(layout(native, field), serialize(getattr(reference, field)))
        initial = torch.load(output / "checkpoint-0.pt", weights_only=False, map_location="cpu")
        self.assertEqual(initial["commands"], 0)
        self.assertEqual(saved["optimizer_steps"], 0)


if __name__ == "__main__":
    unittest.main()
