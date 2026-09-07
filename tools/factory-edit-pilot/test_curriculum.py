"""Curriculum admission, native concurrency, stream boundaries and interrupted evidence."""

from concurrent.futures import ThreadPoolExecutor
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

from curriculum import Collector, prefixes, remaining_at
from curriculum_study import load, train
from native import Runtime, snapshot
from policy import Policy, advantages, batch, single
from reference.acceptance import Catalog
from reference.environment import Environment, serialize

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


@unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build required")
class CurriculumTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.library = Path(os.environ["AIC_NATIVE_LIBRARY"]).resolve()
        self.config = json.loads((DIRECTORY / "run-server-cuda.json").read_text())
        self.config.update(device="cpu", threads=1, horizon=8, evaluation_seeds=2, epochs=1, minibatch=8)
        (self.directory / "config.json").write_text(json.dumps(self.config))
        self.protocol = json.loads((DIRECTORY / "curriculum-parallel-study.json").read_text())
        self.protocol.update(base_config=str(self.directory / "config.json"), commands=64, streams=2,
                             training_seeds=[17], checkpoints=[0, 16, 32, 48, 64],
                             curriculum=[{"until": 16, "remaining": 1}, {"until": 32, "remaining": 2},
                                         {"until": 48, "remaining": 3}])
        self.protocol_path = self.directory / "protocol.json"
        self.protocol_path.write_text(json.dumps(self.protocol))
        torch.set_num_threads(1)
        self.runtimes = load(ROOT, self.protocol_path, "curriculum-parallel", 17, self.library)[3]

    def test_prefix_replays_with_reference_observation_and_reward_parity(self):
        for runtime in self.runtimes.values():
            starts = prefixes(runtime)
            self.assertEqual(len(starts[1]), len(starts[2]) + 1)
            for remaining, prefix in starts.items():
                native = runtime.new_engine()
                native.reset_with_prefix_json(json.dumps(prefix))
                reference = Environment(Catalog(ROOT, runtime.path), self.config)
                for command in prefix:
                    self.assertEqual(reference.step(command)[0], 0)
                reference.steps, reference.revision = 0, 0
                reference.trace.clear()
                reference.episode_counts.clear()
                expected = reference.observe(reference.candidates())
                actual, handle = native.observe()
                for key in expected:
                    self.assertEqual(actual[key].tobytes(), expected[key].tobytes(), (remaining, key))
                self.assertEqual(snapshot(native)["steps"], 0)
                if remaining:
                    commands = runtime.case["witness"][len(prefix):len(prefix)+remaining]
                    total = 0
                    for command in commands:
                        handle, _ = native.candidates()
                        domain = json.loads(native.export_candidates_json(handle))
                        a = native.step(handle, domain.index(command))
                        b = reference.step(command)
                        self.assertEqual(a, b)
                        total += a[0]
                    self.assertGreater(total, 0)

    def test_bad_prefix_is_atomic_and_handles_are_invalidated(self):
        runtime = self.runtimes["B"]
        env = runtime.new_engine()
        env.reset_with_prefix_json(json.dumps(prefixes(runtime)[1]))
        before = snapshot(env)
        handle, _ = env.candidates()
        with self.assertRaisesRegex(ValueError, "curriculum.prefix_state"):
            env.reset_with_prefix_json(json.dumps(runtime.case["witness"][:3]))
        after = snapshot(env)
        for key in ("draft", "base", "best", "steps", "revision", "trace", "episode_counts"):
            self.assertEqual(before[key], after[key], key)
        with self.assertRaisesRegex(ValueError, "stale_handle"):
            env.step(handle, 0)
        with self.assertRaisesRegex(ValueError, "curriculum.prefix_domain"):
            env.reset_with_prefix_json(json.dumps([{"op": "ERASE_TRANSPORT", "network": "battery", "cells": [[999, 999]]}]))

    def test_threaded_native_steps_and_observations_are_byte_identical(self):
        serial = [runtime.new_engine() for runtime in self.runtimes.values()]
        threaded = [runtime.new_engine() for runtime in self.runtimes.values()]
        rng = np.random.default_rng(91)
        with ThreadPoolExecutor(max_workers=2) as pool:
            for _ in range(self.config["horizon"]):
                left = [env.observe() for env in serial]
                right = list(pool.map(lambda env: env.observe(), threaded))
                for a, b in zip(left, right):
                    for key in a[0]:
                        self.assertEqual(a[0][key].tobytes(), b[0][key].tobytes())
                actions = [int(rng.integers(len(pair[0]["candidates"]))) for pair in left]
                a = [env.step(pair[1], action) for env, pair, action in zip(serial, left, actions)]
                b = list(pool.map(lambda item: item[0].step(item[1][1], item[2]), zip(threaded, right, actions)))
                self.assertEqual(a, b)

    def test_complete_streams_keep_gae_separate_and_batch_padding_invariant(self):
        for parallel in (False, True):
            collector = Collector(self.runtimes, self.config, 2, 17, parallel)
            try:
                model = Policy(collector.envs[0].observe()[0])
                rollout, streams = collector.collect(model, 1)
                expected = np.concatenate([advantages(row["rewards"], row["values"], row["dones"], 0, 1, .95)[0]
                                           for row in streams])
                actual = advantages(rollout["rewards"], rollout["values"], rollout["dones"], 0, 1, .95)[0]
                np.testing.assert_array_equal(actual, expected)
                self.assertEqual([i for i, done in enumerate(rollout["dones"]) if done], [7, 15])
                observations = [stream["observations"][0] for stream in streams]
                with torch.no_grad():
                    logits, values = model(batch(observations))
                    for i, observation in enumerate(observations):
                        a, b = model(single(observation))
                        torch.testing.assert_close(logits[i, :len(observation["candidates"])], a[0])
                        torch.testing.assert_close(values[i], b[0])
            finally:
                collector.close()

    def test_interruption_retains_all_streams_rng_and_rollout(self):
        # Witness verification needs the full 21-command B witness.
        config = deepcopy(self.config)
        config["horizon"] = 32
        (self.directory / "config.json").write_text(json.dumps(config))
        protocol = deepcopy(self.protocol)
        protocol.update(commands=256, checkpoints=[0, 64, 128, 192, 256],
                        curriculum=[{"until": 64, "remaining": 1}, {"until": 128, "remaining": 2}, {"until": 192, "remaining": 3}])
        self.protocol_path.write_text(json.dumps(protocol))
        output = self.directory / "interrupted"
        with redirect_stdout(io.StringIO()), patch("curriculum_study.update", side_effect=RuntimeError("test interruption")):
            with self.assertRaisesRegex(RuntimeError, "test interruption"):
                train(ROOT, self.protocol_path, "curriculum-parallel", 17, self.library, output)
        saved = torch.load(output / "interrupted-state.pt", map_location="cpu", weights_only=False)
        self.assertEqual(saved["phase"], "update")
        self.assertEqual(saved["metadata"]["completed_commands"], 64)
        self.assertEqual(len(saved["stream_rngs"]), 2)
        self.assertEqual(len(saved["partial_rollout"]["actions"]), 64)
        for key, state in zip(self.runtimes, saved["environments"]):
            runtime = Runtime(ROOT, self.runtimes[key].path, config, self.library)
            env = runtime.new_engine()
            env.restore_state_json(json.dumps(state))
            self.assertEqual(snapshot(env)["draft"], state["draft"])
        self.assertEqual([remaining_at(protocol, "curriculum-parallel", n) for n in (0, 64, 128, 192)], [1, 2, 3, 0])


if __name__ == "__main__":
    unittest.main()
