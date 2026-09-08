"""A stronger aggregate must not hide a weak original seed or worse geometry."""

from copy import deepcopy
from pathlib import Path
import unittest

from entity_restart_comparison import decide, identity, require_protocol
from mastery_policy_selection import read


class EntityRestartTests(unittest.TestCase):
    def setUp(self):
        self.spec = read(Path(__file__).with_name('entity-restart-comparison.json'))
        self.cells = [{'actor': identity(a), 'horizon': h, 'seed_start': bank, 'case': 'case',
            'translated_successes': 30, 'initial_score': [100, 10, 5], 'median_score': [80, 8, 3],
            'charged_commands': 4096, 'resets_after_initial': 32*(128//h-1)}
            for a in self.spec['actors'] for h in (128, 32) for bank in self.spec['seed_banks']]

    def test_admission_keeps_all_final_seeds_and_supplemental_identity(self):
        require_protocol(self.spec)
        altered = deepcopy(self.spec)
        altered['actors'].pop(2)
        with self.assertRaisesRegex(ValueError, 'entity_restart.protocol'):
            require_protocol(altered)

    def test_supplemental_pass_cannot_replace_weak_final_seed(self):
        self.assertTrue(decide(self.cells, self.spec, ['case'])['final_widths']['1']['restart_candidate_for_transfer'])
        next(c for c in self.cells if c['actor'] == 'width1-seed43-commands3145728' and c['horizon'] == 32)['translated_successes'] = 25
        result = decide(self.cells, self.spec, ['case'])
        self.assertFalse(result['final_widths']['1']['restart_candidate_for_transfer'])
        self.assertTrue(result['actors']['width1-seed43-commands1179648']['methods']['32']['qualified'])

    def test_success_and_geometry_both_matter_and_area_has_priority(self):
        cell = next(c for c in self.cells if c['horizon'] == 32)
        cell.update(translated_successes=32, median_score=[81, 0, 0])
        self.assertFalse(decide(self.cells, self.spec, ['case'])['final_widths']['1']['restart_candidate_for_transfer'])
        cell['median_score'] = [79, 100, 100]
        self.assertTrue(decide(self.cells, self.spec, ['case'])['final_widths']['1']['restart_candidate_for_transfer'])

    def test_complete_matrix_and_charged_budget_required(self):
        for values in (self.cells[:-1], self.cells+[self.cells[0]]):
            with self.assertRaisesRegex(ValueError, 'entity_restart.matrix'):
                decide(values, self.spec, ['case'])
        self.cells[0]['charged_commands'] -= 1
        with self.assertRaisesRegex(ValueError, 'entity_restart.accounting'):
            decide(self.cells, self.spec, ['case'])
