"""Component weighting, case pooling and local derivative interpretation."""

import unittest

import torch

from gradient_contributions import cosine, losses, summarize


class GradientTests(unittest.TestCase):
    def test_components_match_direct_objective_and_finite_difference(self):
        theta = torch.tensor([0.2, -0.1, 0.3], dtype=torch.float64, requires_grad=True)
        features = torch.tensor([[1., 2.], [3., -1.], [-2., 1.], [0.5, -0.5]], dtype=torch.float64)
        actions = torch.tensor([0, 1, 1, 0])
        advantages = torch.tensor([1., -1., 0.5, -0.5], dtype=torch.float64)
        returns = torch.tensor([0.2, -0.4, 0.1, 0.3], dtype=torch.float64)
        config = {'clip': 0.2, 'value_coefficient': 0.4, 'entropy_coefficient': 0.03}

        def outputs(t):
            score = features@t[:2]
            return torch.stack((score, -score), 1), features[:, 0]*t[2]

        old = outputs(theta)[0].log_softmax(-1).gather(1, actions[:, None]).squeeze(1).detach()

        def objective(t, selected):
            logits, values = outputs(t)
            return losses(logits[selected], values[selected], actions[selected], old[selected],
                          advantages[selected], returns[selected], config)

        full = objective(theta, slice(None))
        current, rehearsal = objective(theta, slice(0, 3)), objective(theta, slice(3, 4))
        self.assertTrue(torch.allclose(full, .75*current+.25*rehearsal, atol=1e-12))
        vectors = torch.stack([torch.autograd.grad(v, theta, retain_graph=True)[0] for v in full])
        total_gradient = torch.autograd.grad(full.sum(), theta, retain_graph=True)[0]
        self.assertTrue(torch.allclose(vectors.sum(0), total_gradient, atol=1e-12))
        logits, values = outputs(theta)
        lp = logits.log_softmax(-1)
        direct = -(advantages*torch.exp(lp[torch.arange(4), actions]-old)).mean()
        direct += .4*(values-returns).square().mean()+.03*(lp.exp()*lp).sum(1).mean()
        self.assertTrue(torch.allclose(full.sum(), direct, atol=1e-12))
        direction = torch.tensor([0.3, -0.2, 0.1], dtype=torch.float64)
        epsilon = 1e-6
        finite = (objective(theta+epsilon*direction, slice(None))-objective(theta-epsilon*direction, slice(None)))/(2*epsilon)
        self.assertTrue(torch.allclose(finite, vectors@direction, atol=1e-8))
        split_gradients = [torch.stack([torch.autograd.grad(v, theta, retain_graph=True)[0] for v in group])
                           for group in (current, rehearsal)]
        self.assertTrue(torch.allclose(vectors, .75*split_gradients[0]+.25*split_gradients[1], atol=1e-12))

    def test_conflict_and_zero_norm_have_explicit_meaning(self):
        gradients = torch.tensor([[[1., 0.], [0., 0.], [0., 0.]],
                                  [[-1., 0.], [0., 2.], [0., 0.]]], dtype=torch.float64)
        result = summarize(gradients, torch.tensor([0.5, -0.25], dtype=torch.float64))
        self.assertEqual(result['cases'][0]['policy_cosine_other_cases'], -1)
        self.assertEqual(result['cases'][0]['component_dot_displacement']['policy'], .5)
        self.assertEqual(result['cases'][1]['component_dot_displacement']['policy'], -.5)
        self.assertIsNone(result['pooled_component_cosines'][0][1])
        self.assertIsNone(cosine(torch.zeros(2), torch.ones(2)))
        with self.assertRaisesRegex(ValueError, 'gradient_contributions.shape'):
            summarize(gradients[:1], torch.ones(2))


if __name__ == '__main__':
    unittest.main()
