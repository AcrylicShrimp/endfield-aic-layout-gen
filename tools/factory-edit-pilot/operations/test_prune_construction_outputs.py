import json
from pathlib import Path
import tempfile
import unittest

from prune_construction_outputs import prune


class CleanupTests(unittest.TestCase):
    def fixture(self, root, slot=0, *, complete=True, accepted=True):
        (root / 'manifest.json').write_text('{}')
        episode = root / f'episode-{slot:08d}'
        admission = episode / 'attempt-0/admission'
        admission.mkdir(parents=True)
        for name in ('construction.json', 'construction.html', 'other.json'):
            (admission / name).write_text('preserve unless explicitly obsolete')
        (episode / 'input.json').write_text('{"actual":true}')
        generation = episode / 'attempt-0/generation.json'
        generation.write_text(json.dumps(dict(status='accepted' if accepted else 'rejected',
            original_attempt=dict(factory_validated=accepted))))
        if complete:
            (episode / 'episode.json').write_text(json.dumps(dict(schema='blueprint-fresh-episode-v1',
                files=dict(input=dict(path=str((episode / 'input.json').relative_to(root)))),
                attempts=[dict(status='accepted' if accepted else 'rejected',
                    generation=dict(path=str(generation.relative_to(root))))])))
        return admission

    def test_only_completed_accepted_reports_are_removed_and_inputs_reload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            good = self.fixture(root)
            failed = self.fixture(root, 1, accepted=False)
            partial = self.fixture(root, 2, complete=False)
            result = prune(root)
            self.assertEqual(result['files_removed'], 2)
            self.assertEqual(result['failed_attempts_preserved'], 1)
            self.assertEqual(result['unpublished_inputs'], 1)
            self.assertTrue((good / 'other.json').exists())
            self.assertTrue((failed / 'construction.json').exists())
            self.assertTrue((partial / 'construction.html').exists())
            self.assertEqual(json.loads((good.parent.parent / 'input.json').read_text()), {'actual': True})
            self.assertEqual(prune(root).get('files_removed', 0), 0)

    def test_symlink_and_explicit_input_targets_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            admission = self.fixture(root)
            target = root / 'irreplaceable.json'
            target.write_text('keep')
            report = admission / 'construction.json'
            report.unlink()
            report.symlink_to(target)
            with self.assertRaisesRegex(ValueError, 'regular file'):
                prune(root)
            self.assertEqual(target.read_text(), 'keep')
            report.unlink()
            report.write_text('actual input')
            marker = admission.parent.parent / 'episode.json'
            value = json.loads(marker.read_text())
            value['files']['input']['path'] = str(report.relative_to(root))
            marker.write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, 'actual input'):
                prune(root)
            self.assertEqual(report.read_text(), 'actual input')


if __name__ == '__main__':
    unittest.main()
