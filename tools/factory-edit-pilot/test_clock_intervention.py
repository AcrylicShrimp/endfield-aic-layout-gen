"""The diagnostic changes only a copied clock feature and resets each wave."""

import unittest

import torch

from clock_intervention import ClampedClock, paired_counts


class Echo(torch.nn.Module):
    def forward(self, data):
        return data


class ClockTests(unittest.TestCase):
    def test_clock_changes_only_first_feature_and_restarts_second_wave(self):
        source = {'stats': torch.tensor([[0.75, 2., 3.], [0.5, 4., 5.]]),
                  'candidates': torch.tensor([7])}
        original = source['stats'].clone()
        actor = ClampedClock(Echo())
        observed = [actor(source) for _ in range(129)]
        self.assertEqual([observed[i]['stats'][0, 0].item() for i in (0, 1, 15, 16, 127, 128)],
                         [1., 15/16, 1/16, 0., 0., 1.])
        self.assertTrue(torch.equal(source['stats'], original))
        for row in observed:
            self.assertTrue(torch.equal(row['stats'][:, 1:], original[:, 1:]))
            self.assertIs(row['candidates'], source['candidates'])
            self.assertEqual(row['stats'][0, 0], row['stats'][1, 0])

    def test_pairs_use_lexicographic_score_and_require_same_seeds(self):
        left = [{'seed': 1, 'score': [10, 8, 4]}, {'seed': 2, 'score': [12, 4, 1]}]
        right = [{'seed': 1, 'score': [11, 2, 0]}, {'seed': 2, 'score': [12, 4, 0]}]
        self.assertEqual(paired_counts(left, right), {'wins': 1, 'ties': 0, 'losses': 1})
        with self.assertRaisesRegex(ValueError, 'sampling seeds differ'):
            paired_counts(left, right[::-1])


if __name__ == '__main__':
    unittest.main()
