"""Spatial masks preserve per-world boundaries, normalization and gradients."""
from copy import deepcopy
import unittest

import numpy as np
import torch

from full_factory_input_contracts import WorldEncoder
from ppo_training_spatial import padded_rasters, encode_padded_raster, prepare_samples, gather_samples, restore_samples


class SpatialTests(unittest.TestCase):
    def setUp(self):
        torch.set_num_threads(1)
        torch.manual_seed(81)
        self.encoder = WorldEncoder(1, 8, (1, 1, 2))
        self.worlds = [dict(raster=np.random.default_rng(i).normal(0, .2, (28, h, w)).astype(np.float32))
                       for i, (h, w) in enumerate(((2, 3), (5, 7)))]

    def test_padded_forward_and_gradients_match_unpadded_with_residual_layer(self):
        reference = deepcopy(self.encoder)
        raster, mask = padded_rasters(self.worlds, 'cpu')
        actual = encode_padded_raster(self.encoder, raster, mask)
        self.assertEqual(float((actual * (1 - mask)).detach().abs().sum()), 0)
        losses, reference_losses = [], []
        for i, world in enumerate(self.worlds):
            expected = reference.encode_raster(torch.as_tensor(world['raster'])[None])[0]
            height, width = world['raster'].shape[1:]
            values = actual[i, :, :height, :width]
            torch.testing.assert_close(values, expected, rtol=3e-5, atol=3e-6)
            weights = torch.randn_like(expected)
            losses.append((values * weights).sum())
            reference_losses.append((expected * weights).sum())
        sum(losses).backward()
        sum(reference_losses).backward()
        for (name, a), (_, b) in zip(self.encoder.named_parameters(), reference.named_parameters()):
            self.assertEqual(a.grad is None, b.grad is None, name)
            if a.grad is not None:
                torch.testing.assert_close(a.grad, b.grad, rtol=5e-5, atol=5e-6, msg=name)

    def test_mask_excludes_larger_companion_and_padding_from_spatial_gathers(self):
        raster, mask = padded_rasters(self.worlds, 'cpu')
        features = encode_padded_raster(self.encoder, raster, mask)
        requests = dict(actions=[(1, np.array([[6, 4], [0, 0], [6, 4]])),
                                 (-1, np.array([[999, 999]])),
                                 (0, np.array([[2, 1], [0, 0]]))],
                        members=[(0, np.empty((0, 2), dtype=np.int64))])
        plans, orders = prepare_samples(self.worlds, [[0, 1]], requests, 'cpu', [raster.shape[2:]])
        parts = {name: [] for name in requests}
        for name, values in gather_samples(features, plans[0]).items():
            parts[name].append(values)
        actual = restore_samples(parts, orders, features)
        expected = torch.stack([features[1, :, 4, 6], features[1, :, 0, 0], features[1, :, 4, 6],
                                torch.zeros(8), features[0, :, 1, 2], features[0, :, 0, 0]])
        torch.testing.assert_close(actual['actions'], expected, rtol=0, atol=0)
        self.assertEqual(actual['members'].shape, (0, 8))
        solo, solo_mask = padded_rasters(self.worlds[:1], 'cpu')
        torch.testing.assert_close(features[0, :, :2, :3], encode_padded_raster(self.encoder, solo, solo_mask)[0],
                                   rtol=3e-5, atol=3e-6)


if __name__ == '__main__':
    unittest.main()
