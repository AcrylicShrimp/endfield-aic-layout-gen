"""Capacity comparison rejects hidden training differences and incomplete replication."""

from copy import deepcopy
import json
from pathlib import Path
import unittest

from entity_learning_matrix import validate_studies

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


class EntityLearningMatrixTests(unittest.TestCase):
    def test_only_model_width_may_differ_and_every_seed_must_run(self):
        spec = json.loads((DIRECTORY/'entity-learning-matrix.json').read_text())
        studies = {key: json.loads((ROOT/path).read_text()) for key, path in spec['studies'].items()}
        configs = {key: json.loads((ROOT/study['base_config']).read_text()) for key, study in studies.items()}
        validate_studies(spec, studies, configs)
        changed = deepcopy(configs)
        changed['2']['epochs'] += 1
        with self.assertRaisesRegex(ValueError, 'learning_contract'):
            validate_studies(spec, studies, changed)
        changed = deepcopy(studies)
        changed['2']['commands'] *= 2
        with self.assertRaisesRegex(ValueError, 'learning_contract'):
            validate_studies(spec, changed, configs)
        changed = deepcopy(spec)
        changed['run_order'][-1] = changed['run_order'][0]
        with self.assertRaisesRegex(ValueError, 'run_order'):
            validate_studies(changed, studies, configs)
