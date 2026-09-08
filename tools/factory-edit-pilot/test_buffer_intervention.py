"""Counterfactual states must preserve weights, classify buffers, and own tensors."""

import unittest

import torch

from buffer_intervention import combine


class Toy(torch.nn.Module):
    def __init__(self):
        super().__init__()
        self.normalizations = torch.nn.ModuleList([torch.nn.BatchNorm2d(2) for _ in range(3)])
        self.head = torch.nn.Linear(2, 1)


class BufferTests(unittest.TestCase):
    def test_swaps_statistics_but_preserves_affine_and_other_weights(self):
        actor = Toy()
        weights = {k: torch.ones_like(v) for k, v in actor.state_dict().items()}
        buffers = {k: torch.full_like(v, 7) for k, v in actor.state_dict().items()}
        mixed = combine(actor, weights, buffers)
        buffer_names = set(dict(actor.named_buffers()))
        for key in mixed:
            self.assertTrue(torch.equal(mixed[key], buffers[key] if key in buffer_names else weights[key]))
            mixed[key].zero_()
            self.assertTrue(torch.all(weights[key] == 1))
            self.assertTrue(torch.all(buffers[key] == 7))

    def test_rejects_unknown_state_and_buffer_keys(self):
        actor = Toy()
        original = actor.state_dict()
        with self.assertRaisesRegex(ValueError, 'unclassified state'):
            combine(actor, {**original, 'unknown': torch.tensor(1)}, original)
        actor.register_buffer('unknown', torch.tensor(1))
        with self.assertRaisesRegex(ValueError, 'unclassified state'):
            combine(actor, actor.state_dict(), actor.state_dict())


if __name__ == '__main__':
    unittest.main()
