import copy
import json
import unittest

import torch
from model import Policy, checkpoint, fit_normalization, pack, restore
from train import screened_choice, validate_data


class PolicyTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(19)
        self.samples = [{"split": "train", "features": {"tokens": [[1., 2., 3.], [2., 3., 4.]],
                        "adjacency": [[0., 1.], [0., 0.]], "actions": [[1., 2.], [3., 4.], [5., 6.]]}}]
        self.norm = fit_normalization(self.samples)
        self.inputs = pack(self.samples, self.norm)

    def test_training_only_normalization(self):
        wrong = copy.deepcopy(self.samples)
        wrong[0]["split"] = "test"
        with self.assertRaises(ValueError):
            fit_normalization(wrong)
        self.assertEqual(self.norm["tokens"]["mean"], [1.5, 2.5, 3.5])

    def test_padding_node_and_action_permutations(self):
        tokens, adjacency, actions, mask = self.inputs
        for architecture in ("graph", "pooled"):
            model = Policy(3, 2, 8, architecture).eval()
            original = model(*self.inputs)
            np, ap = torch.tensor([1, 0]), torch.tensor([2, 0, 1])
            changed = model(tokens[:, np], adjacency[:, np][:, :, np], actions[:, ap], mask[:, np])
            torch.testing.assert_close(changed, original[:, ap], atol=1e-6, rtol=1e-5)
            padded = model(torch.cat([tokens, torch.zeros(1, 2, 3)], 1),
                           torch.nn.functional.pad(adjacency, (0, 2, 0, 2)), actions,
                           torch.cat([mask, torch.zeros(1, 2, dtype=torch.bool)], 1))
            torch.testing.assert_close(original, padded, atol=1e-6, rtol=1e-5)

    def test_gradient_signal_zero_control_and_json(self):
        model = Policy(3, 2, 8)
        optimizer = torch.optim.Adam(model.parameters(), lr=.01)
        initial = copy.deepcopy(model.state_dict())
        loss = -model(*self.inputs).log_softmax(-1)[0, 0] * 0
        loss.backward()
        optimizer.step()
        for k, v in model.state_dict().items():
            self.assertTrue(torch.equal(v, initial[k]))
        before = float(model(*self.inputs).softmax(-1)[0, 0].detach())
        for _ in range(20):
            optimizer.zero_grad(set_to_none=True)
            loss = -model(*self.inputs).log_softmax(-1)[0, 0]
            loss.backward()
            optimizer.step()
        after = float(model(*self.inputs).softmax(-1)[0, 0].detach())
        self.assertGreater(after, before + .2)
        restored = restore(json.loads(json.dumps(checkpoint(model, self.norm))))
        self.assertTrue(torch.equal(model(*self.inputs), restored(*self.inputs)))

    def test_inputs_exclude_other_sample_fields(self):
        modified = copy.deepcopy(self.samples)
        modified[0].update(reward=999, flow=999, validator_status="best", id=12345)
        actual = pack(modified, self.norm)
        for before, after in zip(self.inputs, actual):
            self.assertTrue(torch.equal(before, after))

    def test_data_schema_rejects_factory_claim(self):
        with self.assertRaises(ValueError):
            validate_data({"schema_version": 1, "factory_validated": True})

    def test_screened_baseline_does_not_read_rewards(self):
        # Schema-level geometry fixture, not hardcoded game balance or facility data.
        base = {"placements": [{"x": 0, "y": 0, "width": 2, "height": 2}], "transport_networks": []}
        actions = [{"pose": {"x": 0, "y": 0, "width": 2, "height": 2}},
                   {"pose": {"x": 3, "y": 0, "width": 2, "height": 2}}]
        f = [0.] * 21
        f[8], f[9], f[14], f[15], f[18] = 2, 0, 2, 1, 1
        sample = {"base": base, "actions": actions, "features": {"actions": [f, f]}, "current": 1,
                  "rewards": [1, 0], "outcomes": [{"accepted": True}, {"accepted": False}]}
        self.assertEqual(screened_choice(sample), 1)


if __name__ == "__main__":
    unittest.main()
