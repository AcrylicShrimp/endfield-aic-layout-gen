"""Frozen split admission rejects changed cases and overlapping outcome banks."""

import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from geometry_augmentation import digest
from geometry_policy_evaluation import admitted_cases, validate_protocol

ROOT = Path(__file__).resolve().parents[2]
PROTOCOL = ROOT / "tools/factory-edit-pilot/geometry-pilot-evaluation.json"


class GeometryEvaluationTests(unittest.TestCase):
    def test_seed_banks_must_be_disjoint(self):
        protocol = json.loads(PROTOCOL.read_text())
        self.assertEqual(validate_protocol(protocol), [(500000, 64), (510000, 32), (520000, 32)])
        protocol["heldout"]["seed_banks"][0] = 500063
        with self.assertRaisesRegex(ValueError, "overlapping"):
            validate_protocol(protocol)
        protocol["heldout"]["seed_banks"][0] = 500064
        validate_protocol(protocol)
        protocol["development"]["required_successes"] = 65
        with self.assertRaisesRegex(ValueError, "invalid development"):
            validate_protocol(protocol)

    def test_frozen_manifest_and_file_content_are_both_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rows = []
            for split in ("train", "development", "heldout"):
                path = root / f"{split}.json"
                path.write_text(json.dumps({"synthetic": split}))
                rows.append({"id": split, "path": path.name, "split": split,
                             "geometry_sha256": split, "case_sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
            manifest = {"status": "passed", "cases": rows}
            (root / "manifest.json").write_text(json.dumps(manifest))
            protocol = {"geometry_directory": ".", "manifest_cases_sha256": digest(rows)}
            self.assertEqual(set(admitted_cases(root, protocol)), {"train", "development", "heldout"})
            changed = copy.deepcopy(manifest)
            changed["cases"][0]["split"] = "heldout"
            (root / "manifest.json").write_text(json.dumps(changed))
            with self.assertRaisesRegex(ValueError, "manifest differs"):
                admitted_cases(root, protocol)
            (root / "manifest.json").write_text(json.dumps(manifest))
            (root / "heldout.json").write_text('{}')
            with self.assertRaisesRegex(ValueError, "changed, duplicate"):
                admitted_cases(root, protocol)


if __name__ == "__main__":
    unittest.main()
