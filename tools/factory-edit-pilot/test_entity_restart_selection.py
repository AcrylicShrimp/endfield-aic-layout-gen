"""Checkpoint retention and transfer gates must not reuse outcome feedback."""

from copy import deepcopy
from pathlib import Path
import unittest

from entity_restart_comparison import identity
from entity_restart_selection import choose_checkpoints, confirm, require_protocol
from mastery_policy_selection import read


class RestartSelectionTests(unittest.TestCase):
    def setUp(self):
        self.spec = read(Path(__file__).with_name('entity-restart-selection.json'))
        self.cells = [{'actor': identity(a), 'case': key, 'seed_start': 1170000,
            'translated_successes': 32, 'charged_commands': 4096, 'resets_after_initial': 96,
            'initial_score': [area, 10, 2], 'median_score': [area//2, 8, 1]}
            for a in self.spec['actors'] for key, area in [('small', 100), ('large', 1000)]]

    def test_complete_grid_and_earliest_exact_quality_tie(self):
        require_protocol(self.spec)
        result = choose_checkpoints(self.cells, self.spec, ['small', 'large'])
        self.assertEqual(result['eligible_widths'], [1, 2])
        self.assertTrue(all(r['selection']['commands'] == 0 for r in result['selections']))
        for changed in (self.cells[:-1], self.cells+[self.cells[0]]):
            with self.assertRaisesRegex(ValueError, 'entity_restart_selection.matrix'):
                choose_checkpoints(changed, self.spec, ['small', 'large'])
        changed = deepcopy(self.spec)
        changed['actors'][0]['commands'] = 32768
        with self.assertRaisesRegex(ValueError, 'entity_restart_selection.protocol'):
            require_protocol(changed)

    def test_every_seed_needs_one_whole_case_qualifier(self):
        for c in self.cells:
            if c['actor'].startswith('width1-seed29-') and c['case'] == 'small':
                c['translated_successes'] = 25
        result = choose_checkpoints(self.cells, self.spec, ['small', 'large'])
        self.assertEqual(result['eligible_widths'], [2])
        self.assertIsNone(next(r for r in result['selections'] if (r['width'], r['seed']) == (1, 29))['selection'])

    def test_exact_normalized_area_precedes_tile_and_turn_tie_breakers(self):
        target = next(c for c in self.cells if c['actor'] == 'width1-seed17-commands393216' and c['case'] == 'large')
        target['median_score'] = [499, 100, 100]
        result = choose_checkpoints(self.cells, self.spec, ['small', 'large'])
        self.assertEqual(result['selections'][0]['selection']['commands'], 393216)
        target['translated_successes'] = 25
        self.assertEqual(choose_checkpoints(self.cells, self.spec, ['small', 'large'])['selections'][0]['selection']['commands'], 0)

    def test_confirmation_finishes_root_and_development_before_single_heldout(self):
        retained = {w: [{'width': w, 'parameters': w*100}] for w in (1, 2)}
        calls, snapshots = [], []

        def evaluate(stage, width, policies):
            self.assertIs(policies, retained[width])
            calls.append((stage, width))
            return [{'passed': stage != 'heldout', 'initial_score': [100, 10, 1], 'median_score': [50, 5, 0]}]

        result = confirm(retained, evaluate, lambda r: snapshots.append(deepcopy(r)))
        self.assertEqual(calls, [('root_confirmation', 1), ('root_confirmation', 2), ('development', 1), ('development', 2), ('heldout', 1)])
        self.assertEqual(result['failed_stage'], 'heldout')
        self.assertEqual(result['confirmation_status'], 'not_met')
        self.assertEqual(snapshots[-1]['selected_width'], 1)
        self.assertEqual(snapshots[-1]['heldout'], {})

    def test_failed_roots_and_empty_results_do_not_open_transfer(self):
        calls = []

        def evaluate(stage, width, policies):
            calls.append(stage)
            return [{'passed': False, 'initial_score': [100, 10, 1], 'median_score': [50, 5, 0]}]

        result = confirm({1: [{'parameters': 100}]}, evaluate, lambda r: None)
        self.assertEqual(result['failed_stage'], 'root_confirmation')
        self.assertEqual(calls, ['root_confirmation'])
        self.assertEqual(confirm({}, evaluate, lambda r: None)['failed_stage'], 'selection')
        self.assertEqual(calls, ['root_confirmation'])
        with self.assertRaisesRegex(ValueError, 'empty'):
            confirm({1: [{'parameters': 100}]}, lambda *args: [], lambda r: None)
