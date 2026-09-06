import unittest

import torch

from policy import Policy, checkpoint, distribution, restore, terminal_advantages
from train import local_advantages, validate_config


class PolicyTests(unittest.TestCase):
    def test_exact_terminal_order_and_ties(self):
        self.assertEqual(terminal_advantages([[10, 99, 9], [11, 0, 0], [10, 99, 9]]).tolist(), [.5, -1, .5])
        self.assertEqual(terminal_advantages([[1, 2, 3], [1, 2, 3]]).tolist(), [0, 0])
        # Both actions of a detour get the same final credit, including its first worse step.
        advantage = terminal_advantages([[130, 9, 3], [150, 1, 0]])
        self.assertEqual(advantage.expand(2, -1).tolist(), [[1, -1], [1, -1]])

    def test_mask_padding_gradients_and_json_restore(self):
        torch.manual_seed(7)
        observations = [dict(features=dict(tokens=[[1., 0.], [0., 1.]], adjacency=[[0., 1.], [0., 0.]],
                                          actions=[[1., 0., 0.], [0., 1., 0.]], action_mask=[True, False])),
                        dict(features=dict(tokens=[[1., 0.]], adjacency=[[0.]],
                                          actions=[[0., 1., 1.]], action_mask=[True]))]
        model = Policy(2, 3, width=8)
        d = distribution(model, observations)
        self.assertEqual(d.probs.tolist(), [[1, 0], [1, 0]])
        (-d.log_prob(torch.tensor([0, 0])).mean()).backward()
        self.assertTrue(all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()))
        rebuilt = restore(checkpoint(model))
        self.assertTrue(torch.equal(d.logits, distribution(rebuilt, observations).logits))

    def test_zero_signal_preserves_all_weights(self):
        torch.manual_seed(3)
        model = Policy(2, 3, width=8)
        before = {k: v.clone() for k, v in model.state_dict().items()}
        optimizer = torch.optim.Adam(model.parameters(), lr=.001)
        obs = [dict(features=dict(tokens=[[1., 0.]], adjacency=[[0.]],
                                  actions=[[0., 1., 1.], [1., 0., 1.]], action_mask=[True, True]))]
        d = distribution(model, obs)
        loss = -(d.log_prob(d.sample()) * 0).mean()
        loss.backward()
        optimizer.step()
        self.assertTrue(all(torch.equal(v, before[k]) for k, v in model.state_dict().items()))

    def test_myopic_signal_distinguishes_invalid_and_temporary_deterioration(self):
        values = local_advantages([dict(accepted=True, before=[150, 1, 0], score=[160, 0, 0]),
                                   dict(accepted=True, before=[150, 1, 0], score=[140, 9, 9]),
                                   dict(accepted=False)])
        self.assertLess(values[0], 0)
        self.assertGreater(values[1], 0)
        self.assertEqual(values[0], values[2])

    def test_invalid_config_and_checkpoint_fail_explicitly(self):
        with self.assertRaises(ValueError):
            validate_config(dict(schema_version=1))
        with self.assertRaises(ValueError):
            restore(dict(schema_version=2))


if __name__ == "__main__":
    unittest.main()
