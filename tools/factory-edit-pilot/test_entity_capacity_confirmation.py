"""Development selection must precede and strictly gate held-out access."""

import unittest

from entity_capacity_confirmation import confirm_stages, select_width, width_metrics


def cell(area=8, initial=10, passed=True, tiles=2, turns=0):
    return {'median_score': [area, tiles, turns], 'initial_score': [initial, 4, 1], 'passed': passed}


class CapacityConfirmationTests(unittest.TestCase):
    def test_equal_weighted_area_and_size_tie_break(self):
        metrics = width_metrics([cell(6), cell(90, 100)], 70)
        self.assertEqual(metrics['quality_exact'][0], '3/4')
        self.assertEqual(select_width({2: {**metrics, 'parameters': 264}, 1: metrics}), 1)
        self.assertEqual(select_width({1: metrics, 2: width_metrics([cell(7)], 264)}), 2)

    def test_one_failed_cell_disqualifies_width(self):
        self.assertIsNone(select_width({1: width_metrics([cell(), cell(passed=False)], 70)}))

    def test_all_development_before_selection_then_only_chosen_width(self):
        events = []
        retained = {1: [(None, None, {'parameters': 70})], 2: [(None, None, {'parameters': 264})]}

        def evaluate(stage, width, policies):
            events.append((stage, width))
            return [cell(8 if width == 1 else 7)]

        result = confirm_stages(retained, evaluate, lambda result: events.append(('selected', result['selected_width'])))
        self.assertEqual(events, [('development', 1), ('development', 2), ('selected', 2), ('heldout', 2)])
        self.assertEqual(result['status'], 'passed')

    def test_failed_development_and_no_qualifiers_never_open_heldout(self):
        for retained in ({}, {1: [(None, None, {'parameters': 70})]}):
            events = []

            def evaluate(stage, width, policies):
                events.append(stage)
                return [cell(passed=False)]

            result = confirm_stages(retained, evaluate, lambda _: None)
            self.assertNotIn('heldout', events)
            self.assertEqual(result['status'], 'not_met')

    def test_heldout_failure_is_not_replaced_by_second_width(self):
        events = []
        retained = {1: [(None, None, {'parameters': 70})], 2: [(None, None, {'parameters': 264})]}

        def evaluate(stage, width, policies):
            events.append((stage, width))
            return [cell(passed=stage == 'development')]

        result = confirm_stages(retained, evaluate, lambda _: None)
        self.assertEqual(result['status'], 'not_met')
        self.assertEqual(events[-1], ('heldout', 1))
        self.assertNotIn(('heldout', 2), events)


if __name__ == '__main__':
    unittest.main()
