"""Policy retention uses actual root mastery and never later outcome scores."""

import unittest

from mastery_policy_selection import first_mastered


def gate(count, a_level=6, a_streak=2, b_streak=2):
    return {"commands": count, "transition": {"after": {
        "A": {"level": a_level, "streak": a_streak}, "B": {"level": 3, "streak": b_streak}}}}


class SelectionTests(unittest.TestCase):
    def test_first_joint_mastery_ignores_later_outcomes(self):
        gates = [gate(1, a_level=3), gate(2, a_streak=1), gate(3), gate(4)]
        gates[-1]["outcome_return"] = 1.0
        self.assertEqual(first_mastered(gates, {"A": 6, "B": 3}, 2), 3)

    def test_no_joint_mastery_has_no_fallback(self):
        self.assertIsNone(first_mastered([gate(1, b_streak=0), gate(2, a_streak=0)],
                                        {"A": 6, "B": 3}, 2))


if __name__ == "__main__":
    unittest.main()
