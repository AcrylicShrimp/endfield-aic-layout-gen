"""Distinguish actual zero-valued evaluations from nullable training history rows."""
import unittest

from blueprint_capacity_workspace import evaluation_rows


class CapacityWorkspaceTests(unittest.TestCase):
    def test_null_training_rows_are_ignored_but_zero_evaluations_are_retained(self):
        initial = {'train/batches': 0, 'train/actions': 0, 'area': .8, 'delta': 0.}
        training = {'train/batches': 4, 'train/actions': 1024, 'area': None, 'delta': None}
        evaluation = {'train/batches': 4, 'train/actions': 1024, 'area': 0., 'delta': -.8}
        self.assertEqual(evaluation_rows(iter([initial, training, evaluation]), 'area', 'delta'),
                         [initial, evaluation])

    def test_partial_evaluation_is_rejected_instead_of_silently_dropped(self):
        with self.assertRaisesRegex(ValueError, 'missing its axes or paired baseline delta'):
            evaluation_rows([{'train/batches': 4, 'train/actions': 1024, 'area': 0., 'delta': None}],
                            'area', 'delta')


if __name__ == '__main__':
    unittest.main()
