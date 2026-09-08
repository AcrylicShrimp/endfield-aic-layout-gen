"""Replication admission preserves the learning contract and frozen geometry split."""

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from geometry_restart_confirmation import evaluation_stages, learning_contract, validate_protocol, verify_training_inputs

DIRECTORY = Path(__file__).resolve().parent


class GeometryConfirmationTests(unittest.TestCase):
    def test_source_matrix_and_sampling_budget_admission(self):
        spec = json.loads((DIRECTORY/'geometry-restart-confirmation.json').read_text())
        self.assertEqual(validate_protocol(spec), [(800000, 64), (810000, 32), (820000, 32)])
        for key, value in [('training_seeds', [17, 17, 43]), ('sources', {}), ('attempt_horizon', 30)]:
            changed = deepcopy(spec)
            changed[key] = value
            with self.assertRaisesRegex(ValueError, 'geometry_confirmation.protocol'):
                validate_protocol(changed)
        spec['heldout']['seed_banks'][0] = 800063
        with self.assertRaisesRegex(ValueError, 'overlapping'):
            validate_protocol(spec)
        spec['heldout']['seed_banks'][0] = 800064
        validate_protocol(spec)

    def test_first_pass_requires_fresh_root_confirmation_before_geometry(self):
        spec = json.loads((DIRECTORY/'geometry-first-pass-confirmation.json').read_text())
        self.assertEqual(evaluation_stages(spec), ('root_confirmation', 'development', 'heldout'))
        self.assertEqual(validate_protocol(spec), [(790000, 64), (800000, 64), (810000, 32), (820000, 32)])
        changed = deepcopy(spec)
        del changed['root_confirmation']
        with self.assertRaisesRegex(ValueError, 'requires independent'):
            validate_protocol(changed)
        changed = deepcopy(spec)
        changed['root_confirmation']['seed_banks'] = [800010]
        with self.assertRaisesRegex(ValueError, 'overlapping'):
            validate_protocol(changed)
        changed = deepcopy(spec)
        changed['selection'] = 'first_joint_root_mastery'
        with self.assertRaisesRegex(ValueError, 'requires independent'):
            validate_protocol(changed)

    def test_replication_changes_only_admitted_seed_and_id(self):
        original = json.loads((DIRECTORY/'geometry-pilot-study.json').read_text())
        replication = json.loads((DIRECTORY/'geometry-replication-study.json').read_text())
        a = {'config': {'seed': 17, 'commands': original['commands']},
             'training_config': {'seed': 17, 'horizon': 16}, 'protocol': original}
        b = {'config': {'seed': 29, 'commands': replication['commands']},
             'training_config': {'seed': 29, 'horizon': 16}, 'protocol': replication}
        self.assertEqual(learning_contract(a), learning_contract(b))
        b['training_config']['horizon'] = 32
        self.assertNotEqual(learning_contract(a), learning_contract(b))

    def test_training_case_identity_and_audited_content_are_both_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, copy = root/'source.json', root/'copy.json'
            source.write_text('{"geometry":1}')
            copy.write_bytes(source.read_bytes())
            value = hashlib.sha256(source.read_bytes()).hexdigest()
            metadata = {'protocol': {'cases': {'A': 'source.json'}},
                        'provenance': {'A': {'files_sha256': {'source.json': value}}}}
            verify_training_inputs(root, metadata, {'renamed-A': copy})
            source.write_text('{"geometry":2}')
            with self.assertRaisesRegex(ValueError, 'training_case'):
                verify_training_inputs(root, metadata, {'renamed-A': copy})
            source.write_bytes(copy.read_bytes())
            copy.write_text('{"geometry":3}')
            with self.assertRaisesRegex(ValueError, 'training_split'):
                verify_training_inputs(root, metadata, {'renamed-A': copy})
