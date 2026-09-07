"""Gate 1 of update-replay-contract.md: exact credit expectations, not efficacy.

Two binary actions give four trajectories (00, 01, 10, 11). Three Bernoulli
logits parameterize the first action and the second action at each prefix.
Enumerate all 4**4 independent four-worker groups, including repeated paths.
No sampling, model, optimizer, or environment execution is needed.

The reference objective is expected terminal tournament reward against a frozen
opponent distribution. Differentiating both players would instead give zero
identically by antisymmetry. The trainer averages over eight actions per group,
so its expected task-loss gradient is -1/2 the trajectory reward gradient.
Entropy, clipping, Adam, and actual factory learning are outside this test.
"""

from itertools import product
import math
import unittest

from train import credit_signals


PATHS = tuple(product((0, 1), repeat=2))
LOGITS = ((0.0, 0.0, 0.0), (-0.7, 0.4, 1.1), (1.3, -1.2, 0.2))
MODES = ("terminal-score", "quality-futility-prefix")
QUALITY = ((80, 10, 2, 0), (85, 12, 3, 0), (82, 11, 3, 0), (88, 13, 4, 0))
QUALITY_PREFIX = ((90, 15, 4, 0), (95, 16, 5, 0))
FUTILITY = ((90, 15, 4, 1), (90, 15, 4, 0), (90, 15, 4, 1), (90, 15, 4, 2))
FUTILITY_PREFIX = ((90, 15, 4, 0), (90, 15, 4, 1))


def probabilities(logits):
    p, q, r = (1 / (1 + math.exp(-value)) for value in logits)
    return ((1 - p) * (1 - q), (1 - p) * q, p * (1 - r), p * r)


def action_scores(logits, path):
    """Analytic gradients of the two selected conditional log probabilities."""
    p, q, r = (1 / (1 + math.exp(-value)) for value in logits)
    a, b = path
    return ((a - p, 0.0, 0.0),
            (0.0, b - q, 0.0) if a == 0 else (0.0, 0.0, b - r))


def records(outcomes, prefixes):
    def entry(outcome):
        return (dict(best_score=list(outcome[:3]), futile=outcome[3]), [], {})

    # At t=0 all workers share a root. At t=1 only the first action is known;
    # in particular 00/01 and 10/11 have identical pre-action observations.
    histories = [[entry((100, 20, 5, 0)), entry(prefixes[a])] for a, _ in PATHS]
    summaries = [dict(best_score=dict(tuple=list(value[:3])),
                      waste=dict(futile=value[3])) for value in outcomes]
    return histories, summaries


def fixed_rewards(outcomes, opponent, mode):
    """Independent oracle: a win is +1, a loss -1, and a tie zero."""
    values = [value[:3] if mode == "terminal-score" else value for value in outcomes]
    return tuple(math.fsum(weight * (1 if own < other else -1 if own > other else 0)
                           for weight, other in zip(opponent, values)) for own in values)


def objective(logits, rewards):
    return math.fsum(weight * reward for weight, reward in zip(probabilities(logits), rewards))


def analytic_gradient(logits, rewards):
    """Differentiate (1-p)*((1-q)*R00+q*R01)+p*((1-r)*R10+r*R11)."""
    p, q, r = (1 / (1 + math.exp(-value)) for value in logits)
    left = (1 - q) * rewards[0] + q * rewards[1]
    right = (1 - r) * rewards[2] + r * rewards[3]
    return (p * (1 - p) * (right - left),
            (1 - p) * q * (1 - q) * (rewards[1] - rewards[0]),
            p * r * (1 - r) * (rewards[3] - rewards[2]))


def finite_difference(logits, rewards):
    # Rewards already integrate the frozen opponent. Never recompute them at
    # the perturbed logits: that would differentiate the other player's policy.
    epsilon = 1e-5
    gradient = []
    for axis in range(3):
        plus, minus = list(logits), list(logits)
        plus[axis] += epsilon
        minus[axis] -= epsilon
        gradient.append((objective(plus, rewards) - objective(minus, rewards)) / (2 * epsilon))
    return tuple(gradient)


def exhaustive_credit(logits, histories, summaries, mode):
    weights = probabilities(logits)
    scores = [action_scores(logits, path) for path in PATHS]
    gradients, terminal_gradients, masses, energies, counts = [], [], [], [], []
    for group in product(range(4), repeat=4):
        mass = math.prod(weights[index] for index in group)
        credit = credit_signals([histories[index] for index in group],
                                [summaries[index] for index in group], mode)
        # Match the task part of train.run: -signal * logp / samples.
        gradient = tuple(-math.fsum(credit["signals"][worker][step] * scores[index][step][axis]
                                    for worker, index in enumerate(group) for step in range(2)) / 8
                         for axis in range(3))
        terminal = tuple(-math.fsum(credit["terminal"][worker] * scores[index][step][axis]
                                    for worker, index in enumerate(group) for step in range(2)) / 8
                         for axis in range(3))
        gradients.append(tuple(mass * value for value in gradient))
        terminal_gradients.append(tuple(mass * value for value in terminal))
        energies.append(mass * math.fsum(value * value for value in gradient))
        counts.append(mass * sum(value != 0 for row in credit["signals"] for value in row))
        masses.append(mass)
    return dict(gradient=tuple(math.fsum(row[axis] for row in gradients) for axis in range(3)),
                terminal_gradient=tuple(math.fsum(row[axis] for row in terminal_gradients)
                                        for axis in range(3)),
                squared_gradient=math.fsum(energies), nonzero_credit=math.fsum(counts),
                mass=math.fsum(masses), groups=len(masses))


class CreditReferenceTests(unittest.TestCase):
    def assert_vector_close(self, actual, expected, tolerance=1e-12):
        self.assertEqual(len(actual), len(expected))
        for axis, (value, reference) in enumerate(zip(actual, expected)):
            self.assertAlmostEqual(value, reference, delta=tolerance, msg=f"gradient axis {axis}")

    def check_reference(self, logits, outcomes, prefixes, mode):
        histories, summaries = records(outcomes, prefixes)
        opponent = probabilities(logits)
        rewards = fixed_rewards(outcomes, opponent, mode)
        analytic = analytic_gradient(logits, rewards)
        self.assert_vector_close(finite_difference(logits, rewards), analytic, 2e-10)
        actual = exhaustive_credit(logits, histories, summaries, mode)
        self.assertEqual(actual["groups"], 256)
        self.assertAlmostEqual(actual["mass"], 1.0, delta=1e-12)
        expected_loss_gradient = tuple(-value / 2 for value in analytic)
        self.assert_vector_close(actual["gradient"], expected_loss_gradient)
        self.assert_vector_close(actual["terminal_gradient"], expected_loss_gradient)
        self.assert_vector_close(actual["gradient"], actual["terminal_gradient"])
        # Self-play value is zero at this point, even when its fixed-opponent
        # derivative is nonzero. Prefix baselines do not redefine this value.
        self.assertAlmostEqual(objective(logits, rewards), 0.0, delta=1e-12)
        return actual

    def test_uniform_case_has_explicit_analytical_gradient(self):
        rewards = fixed_rewards(QUALITY, (0.25,) * 4, "quality-futility-prefix")
        self.assertEqual(rewards, (0.75, -0.25, 0.25, -0.75))
        self.assertEqual(analytic_gradient(LOGITS[0], rewards), (-0.125,) * 3)
        actual = self.check_reference(LOGITS[0], QUALITY, QUALITY_PREFIX, "quality-futility-prefix")
        self.assert_vector_close(actual["gradient"], (0.0625,) * 3)

    def test_pre_action_quality_and_futility_baselines_are_unbiased(self):
        for logits, mode, (outcomes, prefixes) in product(
                LOGITS, MODES, ((QUALITY, QUALITY_PREFIX), (FUTILITY, FUTILITY_PREFIX))):
            with self.subTest(logits=logits, mode=mode, outcomes=outcomes):
                histories, _ = records(outcomes, prefixes)
                self.assertEqual(histories[0], histories[1])
                self.assertEqual(histories[2], histories[3])
                self.assertNotEqual(histories[0][1], histories[2][1])
                actual = self.check_reference(logits, outcomes, prefixes, mode)
                if outcomes == FUTILITY and mode == "terminal-score":
                    self.assert_vector_close(actual["gradient"], (0.0,) * 3)
                else:
                    self.assertGreater(max(abs(value) for value in actual["gradient"]), 1e-4)

    def test_quality_priority_then_futility_then_exact_tie(self):
        cases = (
            ((80, 99, 99, 2), (81, 0, 0, 0), True, True),
            ((80, 10, 99, 2), (80, 11, 0, 0), True, True),
            ((80, 10, 2, 2), (80, 10, 3, 0), True, True),
            ((80, 10, 2, 1), (80, 10, 2, 2), False, True),
            ((80, 10, 2, 1), (80, 10, 2, 1), False, False),
        )
        for better, worse, quality_differs, combined_differs in cases:
            for mode in MODES:
                with self.subTest(better=better, worse=worse, mode=mode):
                    outcomes = (better, worse, better, worse)
                    histories, summaries = records(outcomes, QUALITY_PREFIX)
                    credit = credit_signals(histories, summaries, mode)
                    differs = quality_differs if mode == "terminal-score" else combined_differs
                    expected = [2 / 3, -2 / 3, 2 / 3, -2 / 3] if differs else [0] * 4
                    self.assertEqual(credit["terminal"], expected)
                    self.assertEqual(credit["outcomes"], [list(v[:3] if mode == "terminal-score" else v)
                                                         for v in outcomes])
                    self.assertEqual(credit["quality_terminal"],
                                     [2 / 3, -2 / 3, 2 / 3, -2 / 3] if quality_differs else [0] * 4)
                    self.assertEqual([row[0] for row in credit["signals"]], expected)

    def test_tied_terminal_nonzero_prefix_and_coefficients_do_not_establish_learning(self):
        for logits, (outcomes, prefixes) in product(LOGITS, (
                (((80, 10, 2, 0),) * 4, QUALITY_PREFIX),
                (((90, 15, 4, 1),) * 4, FUTILITY_PREFIX))):
            with self.subTest(logits=logits, prefixes=prefixes):
                histories, summaries = records(outcomes, prefixes)
                credit = credit_signals(histories, summaries, "quality-futility-prefix")
                self.assertEqual(credit["terminal"], [0] * 4)
                self.assertEqual(credit["signals"], [[0, -2 / 3], [0, -2 / 3],
                                                     [0, 2 / 3], [0, 2 / 3]])
                actual = self.check_reference(logits, outcomes, prefixes, "quality-futility-prefix")
                self.assertGreater(actual["nonzero_credit"], 0)
                self.assertGreater(actual["squared_gradient"], 0)
                for coefficient in (0.0, 1.0, 7.0):
                    self.assert_vector_close(tuple(coefficient * value for value in actual["gradient"]),
                                             (0.0,) * 3)
                # Individual updates can be nonzero and scaled up while the
                # terminal objective remains identically zero for every policy.
                rewards = fixed_rewards(outcomes, probabilities(logits), "quality-futility-prefix")
                for candidate in LOGITS:
                    self.assertEqual(objective(candidate, rewards), 0.0)

    def test_post_action_leakage_is_a_biased_negative_control(self):
        histories, summaries = records(QUALITY, QUALITY_PREFIX)
        for history, outcome in zip(histories, QUALITY):
            for step in range(2):
                history[step] = (dict(best_score=list(outcome[:3]), futile=outcome[3]), [], {})
        actual = exhaustive_credit(LOGITS[0], histories, summaries, "quality-futility-prefix")
        self.assert_vector_close(actual["gradient"], (0.0,) * 3)
        self.assert_vector_close(actual["terminal_gradient"], (0.0625,) * 3)
        self.assertNotEqual(actual["gradient"], actual["terminal_gradient"])


if __name__ == "__main__":
    unittest.main()
