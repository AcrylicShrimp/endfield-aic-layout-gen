"""Snapshot-based hypothetical work must remain charged after rollback."""

import os
from pathlib import Path
import unittest

from mastery_policy_selection import read
from native import Runtime
from tree_search_cost import probe


@unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'explicit native build required')
class TreeCostTests(unittest.TestCase):
    def setUp(self):
        root = Path(__file__).resolve().parents[2]
        directory = root/'tools/factory-edit-pilot'
        self.runtime = Runtime(root, directory/'case.json', {**read(directory/'run.json'), 'horizon': 4},
                               os.environ['AIC_NATIVE_LIBRARY'])

    def test_rollbacks_do_not_erase_simulated_work(self):
        result = probe(self.runtime, 3, 4, 17, 30, 1048576)
        self.assertEqual(result['simulated_commands'], 12)
        self.assertEqual(result['counts']['restore'], 15)
        self.assertEqual(result['executed_commands'], 0)
        self.assertEqual(len(result['branching']), 12)
        self.assertTrue(result['original_engine_unchanged'])

    def test_snapshot_byte_limit_stops_before_transition(self):
        result = probe(self.runtime, 3, 4, 17, 30, 1)
        self.assertEqual(result['termination'], 'snapshot_byte_budget')
        self.assertEqual(result['simulated_commands'], 0)


if __name__ == '__main__':
    unittest.main()
