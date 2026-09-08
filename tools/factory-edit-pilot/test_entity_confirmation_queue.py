"""A stopped process must never bypass the complete audited-matrix prerequisite."""

import json
from pathlib import Path
import tempfile
import unittest

from entity_confirmation_queue import require_complete


class MatrixGateTests(unittest.TestCase):
    def test_rejects_missing_or_reordered_audits_even_when_status_says_complete(self):
        order = [{'width': 1, 'seed': 17}, {'width': 2, 'seed': 17}]
        rows = [{**row, 'status': 'complete_and_audited'} for row in order]
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            for bad in (rows[:1], rows[::-1], [{**rows[0], 'status': 'running'}, rows[1]]):
                (source/'matrix.json').write_text(json.dumps(
                    {'status': 'complete', 'protocol': {'run_order': order}, 'runs': bad}))
                with self.assertRaisesRegex(ValueError, 'complete audited matrix'):
                    require_complete(source)
            valid = {'status': 'complete', 'protocol': {'run_order': order}, 'runs': rows}
            (source/'matrix.json').write_text(json.dumps(valid))
            self.assertEqual(require_complete(source), valid)

    def test_rejects_interrupted_matrix_with_finished_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            (source/'matrix.json').write_text(json.dumps({'status': 'interrupted',
                'protocol': {'run_order': [{'width': 1, 'seed': 17}]},
                'runs': [{'width': 1, 'seed': 17, 'status': 'complete_and_audited'}]}))
            with self.assertRaises(ValueError):
                require_complete(source)


if __name__ == '__main__':
    unittest.main()
