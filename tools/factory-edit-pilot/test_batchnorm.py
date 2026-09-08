"""Masked BN conventions, frozen PPO likelihoods and training-only calibration."""

from copy import deepcopy
import json
from pathlib import Path
import unittest
from unittest.mock import patch

import numpy as np
import torch
from torch import nn

from policy import MaskedBatchNorm2d, Policy, batch, update
from reference.acceptance import Catalog
from reference.environment import Environment
from run import refresh_actor

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class BatchNormTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        self.config = json.loads((DIRECTORY / "run-server-cuda.json").read_text())
        self.config.update(device="cpu", horizon=8, epochs=2, minibatch=4, normalization="batch")
        self.catalog = Catalog(ROOT, DIRECTORY / "case.json")
        self.env = Environment(self.catalog, self.config)
        self.observation = self.env.observe(self.env.candidates())

    def test_masked_calibration_matches_packed_pytorch_batchnorm(self):
        torch.manual_seed(9)
        data = torch.randn(2, 3, 4, 5)
        valid = torch.zeros(2, 1, 4, 5)
        valid[0, :, :2, :3] = 1
        valid[1, :, :3, :4] = 1
        model = MaskedBatchNorm2d(3)
        packed = data.permute(1, 0, 2, 3).reshape(3, -1)[:, valid.flatten().bool()][None]
        reference = nn.BatchNorm1d(3)
        with torch.no_grad():
            wanted = reference(packed)
            actual = model(data, valid, calibrate=True)
        selected = actual.permute(1, 0, 2, 3).reshape(3, -1)[:, valid.flatten().bool()][None]
        torch.testing.assert_close(selected, wanted)
        torch.testing.assert_close(model.running_mean, reference.running_mean)
        torch.testing.assert_close(model.running_var, reference.running_var)
        padded = nn.functional.pad(data, (0, 7, 0, 6), value=1234)
        padded_valid = nn.functional.pad(valid, (0, 7, 0, 6))
        alternative = MaskedBatchNorm2d(3)
        with torch.no_grad():
            alternative(padded, padded_valid, calibrate=True)
        torch.testing.assert_close(alternative.running_mean, model.running_mean)
        torch.testing.assert_close(alternative.running_var, model.running_var)

    def test_frozen_inference_padding_initialization_and_actor_state(self):
        torch.manual_seed(17)
        control = Policy(self.observation)
        torch.manual_seed(17)
        model = Policy(self.observation, normalization="batch")
        for name, value in control.state_dict().items():
            torch.testing.assert_close(value, model.state_dict()[name], rtol=0, atol=0)
        second = deepcopy(self.observation)
        second["raster"] = np.pad(second["raster"], ((0, 0), (0, 3), (0, 5)))
        model.calibrate_normalization([self.observation, second], 2)
        before = {name: value.clone() for name, value in model.named_buffers()}
        model.train()
        solo, solo_value = model(batch([self.observation]))
        together, together_value = model(batch([self.observation, second]))
        torch.testing.assert_close(solo[0], together[0], atol=1e-6, rtol=1e-5)
        torch.testing.assert_close(solo_value[0], together_value[0], atol=1e-6, rtol=1e-5)
        for name, value in model.named_buffers():
            torch.testing.assert_close(value, before[name], rtol=0, atol=0)
        actor = Policy(self.observation, normalization="batch")
        refresh_actor(model, actor)
        actor.eval()
        actual = actor(batch([self.observation]))
        torch.testing.assert_close(actual[0], solo)
        torch.testing.assert_close(actual[1], solo_value)
        with self.assertRaisesRegex(ValueError, "policy.normalization"):
            Policy(self.observation, normalization="invalid")

    def test_ppo_freezes_statistics_until_one_calibration_pass(self):
        model = Policy(self.observation, normalization="batch")
        rollout = {key: [] for key in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
        for _ in range(8):
            domain = self.env.candidates()
            observation = self.env.observe(domain)
            action, log_prob, value = model.act(observation)
            reward, done, _ = self.env.step(domain.to_dicts()[action])
            for key, value in zip(rollout, (observation, action, log_prob, value, reward, done)):
                rollout[key].append(value)
        with torch.no_grad():
            logits, _ = model(batch(rollout["observations"]))
            actual = logits.log_softmax(-1).gather(1, torch.tensor(rollout["actions"])[:, None]).squeeze(1)
            torch.testing.assert_close(actual, torch.tensor(rollout["log_probs"]), atol=1e-6, rtol=1e-5)
        before = {name: value.clone() for name, value in model.named_buffers()}
        original = model.calibrate_normalization
        def calibrate(observations, size):
            for name, value in model.named_buffers():
                torch.testing.assert_close(value, before[name], rtol=0, atol=0)
            return original(observations, size)
        optimizer = torch.optim.Adam(model.parameters(), lr=self.config["learning_rate"])
        with patch.object(model, "calibrate_normalization", side_effect=calibrate) as called:
            metrics = update(model, optimizer, rollout, self.config, np.random.default_rng(17))
        self.assertEqual(called.call_count, 1)
        self.assertEqual(metrics["optimizer_steps"], 4)
        self.assertEqual(metrics["normalization_calibration"]["observations"], 8)
        self.assertEqual(metrics["normalization_calibration"]["batches"], 2)
        self.assertTrue(all(layer.num_batches_tracked.item() == 2 for layer in model.normalizations))
        self.assertTrue(any(not torch.equal(value, before[name]) for name, value in model.named_buffers()))
        self.assertTrue(all(torch.isfinite(value).all() for value in model.state_dict().values()))


if __name__ == "__main__":
    unittest.main()
