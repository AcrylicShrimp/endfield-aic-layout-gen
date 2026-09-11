"""Historical cohort identity must survive export and local relocation unchanged."""
from pathlib import Path
import tempfile
import unittest

from blueprint_evaluation_cohort import load_cohort
from export_blueprint_evaluation_cohort import export
from ppo_harness_state import read, sha, write


class CohortTests(unittest.TestCase):
    def fixture(self, root):
        catalog = root / 'catalog.json'
        write(catalog, {'version': 1})
        entries = {}
        for seed in (21, 22):
            files = {}
            for name in ('request', 'initial', 'original'):
                path = root / str(seed) / f'{name}.json'
                path.parent.mkdir(exist_ok=True)
                write(path, dict(seed=seed, input=name))
                files[name] = dict(path=str(path), sha256=sha(path))
            entries[str(seed)] = dict(status='accepted', generation_seed=seed, files=files,
                                      generation=dict(initial_metrics=dict(score=[100, 10, 1])))
        protocol = root / 'protocol.json'
        write(protocol, dict(config=dict(seeds=[1], training=dict(horizon=32),
            evaluation=dict(attempts=2, action_seed_base=1000)),
            spec=dict(catalogs=dict(test=str(catalog)), frozen_starts=dict(entries=entries, evaluation_seeds=[21, 22]))))
        return protocol, dict(test=catalog)

    def test_export_keeps_case_order_bytes_and_original_action_seeds(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            protocol, catalogs = self.fixture(root)
            output = root / 'cohort'
            export(protocol, output, sha(protocol))
            cases, sources = load_cohort(output / 'cohort.json', 2, catalogs)
            self.assertEqual([c['generation_seed'] for c in cases], [21, 22])
            self.assertEqual([c['action_seed'] for c in cases], [101000, 101001])
            self.assertEqual(len(sources), 8)
            for case in cases:
                for ref in case['files'].values():
                    self.assertEqual((output / ref['path']).read_bytes(), Path(ref['source_path']).read_bytes())
            with self.assertRaisesRegex(ValueError, 'cardinality|complete historical'):
                load_cohort(output / 'cohort.json', 32, catalogs)

    def test_changed_input_and_cohort_order_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            protocol, catalogs = self.fixture(root)
            output = root / 'cohort'
            export(protocol, output, sha(protocol))
            manifest = output / 'cohort.json'
            value = read(manifest)
            value['cases'].reverse()
            write(manifest, value)
            with self.assertRaisesRegex(ValueError, 'order'):
                load_cohort(manifest, 2, catalogs)
            value['cases'].reverse()
            write(manifest, value)
            (output / '21/initial.json').write_text('{}')
            with self.assertRaisesRegex(ValueError, 'unchanged cohort input'):
                load_cohort(manifest, 2, catalogs)


if __name__ == '__main__':
    unittest.main()
