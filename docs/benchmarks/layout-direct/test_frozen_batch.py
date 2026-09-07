import copy
import json
import math
from itertools import product
from pathlib import Path
import unittest

from frozen_batch import bootstrap, gradient_statistics, paired_quality, seed, signals_for, validate_config
from test_credit_reference import (LOGITS, PATHS, QUALITY, QUALITY_PREFIX, FUTILITY, FUTILITY_PREFIX,
                                   action_scores, exhaustive_credit, probabilities, records)
from train import credit_signals


class FrozenBatchTests(unittest.TestCase):
    def test_all_action_stream_roles_and_batches_are_distinct(self):
        values = [seed(role, root, batch, index) for root in range(2) for batch in range(8)
                  for role, index in [("behavior", 0), *[("evaluation", i) for i in range(4)]]]
        self.assertEqual(len(values), len(set(values)))
        self.assertEqual(seed("evaluation", 1, 2, 3), seed("evaluation", 1, 2, 3))

    def test_fixed_credit_controls_preserve_sequences_and_terminal_outcomes(self):
        credit = dict(signals=[[1, 0], [-1, .5], [.5, -1], [-.5, .5]], terminal=[1, -1, .5, -.5])
        original = copy.deepcopy(credit)
        arms = signals_for(credit, [[0, 0]] * 4)
        self.assertEqual(arms["terminal_same_outcomes"], [[1, 1], [-1, -1], [.5, .5], [-.5, -.5]])
        self.assertEqual(arms["shuffled"], [credit["signals"][i] for i in (1, 2, 3, 0)])
        self.assertEqual(arms["zero"], [[0, 0]] * 4)
        self.assertEqual(credit, original)

    def test_batch_covariance_and_undefined_directions(self):
        stats = gradient_statistics([[1, 0], [-1, 0]])
        self.assertEqual(stats["sample_covariance_trace"], 2)
        self.assertEqual(stats["mean_norm"], 0)
        self.assertEqual(stats["leave_one_out_cosines"], [-1, -1])
        zero = gradient_statistics([[0, 0], [0, 0]])
        self.assertIsNone(zero["mean_norm_over_rms_dispersion"])
        self.assertEqual(zero["leave_one_out_cosines"], [None, None])
        with self.assertRaises(ValueError):
            gradient_statistics([[1, 2]])

    def test_prospective_pairing_preserves_lexicographic_priority(self):
        def episode(score, action_seed=1):
            return dict(seed=action_seed, summary=dict(root="complete-root", work=dict(proposals=128),
                        best_score=dict(tuple=score), waste=dict(futile_fraction=.8)))
        control = [episode([100, 10, 3])]
        good = [episode([99, 9999, 9999])]
        result = paired_quality(good, control)
        self.assertEqual(result["mean_win"], 1)
        self.assertEqual(result["mean_delta"], [-1, 9989, 9996])
        self.assertEqual(paired_quality(control, good)["mean_win"], -1)
        self.assertEqual(paired_quality(control, control)["mean_win"], 0)
        with self.assertRaises(ValueError):
            paired_quality([episode([99, 9, 3], 2)], control)

    def test_bootstrap_unit_and_frozen_allocation(self):
        result = bootstrap([.5] * 8, 2000, 42)
        self.assertEqual(result["percentile_95"], [.5, .5])
        self.assertEqual(result["unit"], "independent-update-batch")
        self.assertEqual(bootstrap([0, 1] * 4, 2000, 42), bootstrap([0, 1] * 4, 2000, 42))
        config = json.loads(Path(__file__).with_name("frozen-batch.json").read_text())
        validate_config(config)
        with self.assertRaises(ValueError):
            validate_config(dict(config, batches=4))

    def test_cyclic_peer_rank_credit_is_not_an_independent_zero_mean_null(self):
        # Peer rank includes the receiving worker's outcome. Under exchangeable
        # independent rollouts, a nonself peer's expected gradient is -1/(K-1)
        # times the correctly assigned gradient, not zero. This is pre-Adam.
        for logits, (outcomes, prefixes) in product(LOGITS, ((QUALITY, QUALITY_PREFIX), (FUTILITY, FUTILITY_PREFIX))):
            histories, summaries = records(outcomes, prefixes)
            weights = probabilities(logits)
            scores = [action_scores(logits, path) for path in PATHS]
            expected = exhaustive_credit(logits, histories, summaries, "quality-futility-prefix")["gradient"]
            contributions = []
            for group in product(range(4), repeat=4):
                mass = math.prod(weights[i] for i in group)
                hs = [histories[i] for i in group]
                credit = credit_signals(hs, [summaries[i] for i in group], "quality-futility-prefix")
                shuffled = signals_for(credit, hs)["shuffled"]
                contributions.append(tuple(-mass * math.fsum(shuffled[w][t] * scores[i][t][axis]
                    for w, i in enumerate(group) for t in range(2)) / 8 for axis in range(3)))
            for axis in range(3):
                self.assertAlmostEqual(math.fsum(g[axis] for g in contributions), -expected[axis] / 3, delta=1e-12)


if __name__ == "__main__":
    unittest.main()
