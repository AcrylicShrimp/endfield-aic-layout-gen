"""Serial request generation preserves raw-input and exact-rate obligations."""

from copy import deepcopy
import json
import os
from pathlib import Path
import tempfile
import unittest

from chain_augmentation import generate, serial_case
from geometry_augmentation import validate

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class ChainAugmentationTests(unittest.TestCase):
    def setUp(self):
        self.spec = json.loads((DIRECTORY/'chain-augmentation.json').read_text())
        self.serial = [v for v in self.spec['templates'].values() if 'recipes' in v]

    def test_no_undeclared_intermediate_import_or_inconsistent_chain(self):
        longer = deepcopy(next(v for v in self.serial if len(v['recipes']) == 3))
        longer['recipes'] = longer['recipes'][1:]
        with self.assertRaisesRegex(ValueError, 'chain_augmentation.external'):
            serial_case(ROOT, self.spec['catalogs'], longer, 'cut')
        changed = deepcopy(self.serial[0])
        changed['recipes'][1] = next(v for v in self.serial if v['recipes'][0] != changed['recipes'][0])['recipes'][1]
        with self.assertRaisesRegex(ValueError, 'chain_augmentation.commodity'):
            serial_case(ROOT, self.spec['catalogs'], changed, 'mixed')
        changed = deepcopy(self.serial[0])
        changed['target_rate'] = [10**9, 1]
        with self.assertRaisesRegex(ValueError, 'chain_augmentation.rate'):
            serial_case(ROOT, self.spec['catalogs'], changed, 'excessive')

    @unittest.skipUnless(os.environ.get('AIC_NATIVE_LIBRARY'), 'explicit native build required')
    def test_two_three_step_witnesses_and_duplicate_split_rejection(self):
        library = Path(os.environ['AIC_NATIVE_LIBRARY']).resolve()
        config = json.loads((ROOT/self.spec['config']).read_text())
        config.update(device='cpu', horizon=128)
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            for index, definition in enumerate(self.serial):
                case = serial_case(ROOT, self.spec['catalogs'], definition, str(index))
                path = directory/f'{index}.json'
                path.write_text(json.dumps(case))
                evidence = validate(ROOT, path, config, library)
                self.assertEqual(evidence['first_reward_distance'], 9)
                self.assertLess(evidence['witness_score'][0], evidence['initial_score'][0])
            duplicate = deepcopy(self.spec)
            group = deepcopy(duplicate['splits']['train'][0])
            group.update(gap_extensions=[0], quarter_turns=[0])
            duplicate['splits'] = {key: [group] for key in ('train', 'development', 'heldout')}
            path = directory/'duplicate.json'
            path.write_text(json.dumps(duplicate))
            output = directory/'output'
            with self.assertRaisesRegex(ValueError, 'chain_augmentation.duplicate'):
                generate(ROOT, path, output, library)
            self.assertFalse(output.exists())
