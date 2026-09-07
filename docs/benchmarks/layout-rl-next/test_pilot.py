import unittest

from pilot import read, validate
from pathlib import Path


class PilotPlanTests(unittest.TestCase):
    def test_equal_action_matrix_and_zero_control(self):
        plan = read(Path(__file__).with_name("pilot.json"))
        validate(plan)
        self.assertEqual([plan["training_actions_per_arm"] // (h * plan["workers"]) for h in plan["horizons"]], [64, 32, 16])
        for override in ({"horizons": [16, 32]}, {"workers": 1}, {"training_actions_per_arm": 8193},
                         {"evaluation_proposals": [128]}, {"zero_control_horizon": 32}):
            with self.assertRaises(ValueError):
                validate({**plan, **override})


if __name__ == "__main__":
    unittest.main()
