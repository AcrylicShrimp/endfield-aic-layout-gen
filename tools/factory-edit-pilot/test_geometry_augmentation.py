"""Geometry augmentation admission and split isolation."""

import copy
import json
import os
from pathlib import Path
import tempfile
import unittest

from geometry_augmentation import generate, transform
from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize

ROOT = Path(__file__).resolve().parents[2]
CASE = ROOT / "tools/factory-edit-pilot/case-relocation.json"
SPEC = CASE.parent / "geometry-augmentation.json"


class GeometryAugmentationTests(unittest.TestCase):
    def test_all_gaps_and_rotations_preserve_valid_relocation(self):
        catalog = Catalog(ROOT, CASE)
        original = copy.deepcopy(catalog.case)
        config = json.loads((CASE.parent / "run-capacity-width1.json").read_text())
        config.update(device="cpu", horizon=16)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "case.json"
            for gap in (0, 2, 4, 6, 8):
                for turns in range(4):
                    with self.subTest(gap=gap, turns=turns):
                        case = transform(catalog, gap, turns)
                        path.write_text(json.dumps(case))
                        derived = Catalog(ROOT, path)
                        env = Environment(derived, config)
                        self.assertTrue(check(derived, serialize(env.best))["valid"])
                        self.assertEqual(env.initial_score[0], 133 + 7 * gap)
                        for index, command in enumerate(case["witness"]):
                            self.assertIn(command, env.candidates().to_dicts())
                            reward, done, outcome = env.step(command)
                            self.assertTrue(outcome["applied"])
                            self.assertFalse(done)
                            self.assertEqual(reward > 0, index == 8)
                        self.assertEqual(env.best_score[0], 119 + 7 * gap)
                        self.assertTrue(check(derived, serialize(env.best))["valid"])
        self.assertEqual(catalog.case, original)

    def test_reject_unsupported_parameters_and_witness(self):
        catalog = Catalog(ROOT, CASE)
        for gap, turns in ((-1, 0), (0, 4), (True, 0), (0, 1.5)):
            with self.assertRaisesRegex(ValueError, "augmentation.parameters"):
                transform(catalog, gap, turns)
        catalog.case["witness"][5]["assignments"]["F"][0] += 1
        with self.assertRaisesRegex(ValueError, "augmentation.witness"):
            transform(catalog, 2, 0)

    @unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build required")
    def test_native_manifest_and_duplicate_split_rejection(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "generated"
            library = Path(os.environ["AIC_NATIVE_LIBRARY"]).resolve()
            manifest = generate(ROOT, SPEC, output, library)
            self.assertEqual(len(manifest["cases"]), 20)
            self.assertEqual({r["curriculum_levels"] for r in manifest["cases"]}, {9})
            self.assertEqual({r["first_reward_distance"] for r in manifest["cases"]}, {9})
            with self.assertRaisesRegex(ValueError, "augmentation.output_exists"):
                generate(ROOT, SPEC, output, library)
            spec = json.loads(SPEC.read_text())
            spec["splits"]["development"]["gap_extensions"] = [0]
            bad = Path(directory) / "bad.json"
            bad.write_text(json.dumps(spec))
            with self.assertRaisesRegex(ValueError, "augmentation.duplicate"):
                generate(ROOT, bad, Path(directory) / "rejected", library)
            self.assertFalse((Path(directory) / "rejected").exists())


if __name__ == "__main__":
    unittest.main()
