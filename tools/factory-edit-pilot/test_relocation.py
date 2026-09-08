"""The selected root requires placement changes and admits a complete relocation."""

import json
import os
from pathlib import Path
import unittest

from native import Runtime, layout
from reference.acceptance import Catalog, bounds, check, footprint, port_geometry
from reference.environment import Environment, serialize

ROOT = Path(__file__).resolve().parents[2]
CASE = ROOT / "tools/factory-edit-pilot/case-relocation.json"


class RelocationTests(unittest.TestCase):
    def setUp(self):
        self.catalog = Catalog(ROOT, CASE)
        self.config = json.loads((CASE.parent / "run-server-cuda-batchnorm.json").read_text())
        self.config.update(device="cpu", horizon=16)

    def test_fixed_pose_area_bound_and_complete_translated_witness(self):
        catalog = self.catalog
        env = Environment(catalog, self.config)
        cells = set().union(*(footprint(catalog, fid, f["pose"]) for fid, f in catalog.facilities.items()))
        x0, y0, x1, y1 = bounds(cells)
        self.assertEqual((x0, y0, x1, y1), (0, 0, 6, 16))
        mandatory_y = {}
        for fid, direction in (("F", "input"), ("T", "output")):
            candidates = [port_geometry(catalog, fid, catalog.facilities[fid]["pose"], p["id"])[0]
                          for p in catalog.facility_type(fid)["ports"]
                          if p["direction"] == direction and p["transport"] == "belt"]
            self.assertTrue(candidates)
            mandatory_y[fid] = {cell[1] for cell in candidates}
        self.assertEqual(mandatory_y, {"F": {17}, "T": {-1}})
        self.assertEqual((x1-x0+1)*(17-(-1)+1), env.initial_score[0])
        for index, command in enumerate(catalog.case["witness"]):
            self.assertIn(command, env.candidates().to_dicts())
            reward, done, outcome = env.step(command)
            self.assertTrue(outcome["applied"])
            self.assertFalse(done)
            self.assertEqual(reward > 0, index == len(catalog.case["witness"])-1)
            self.assertTrue(check(catalog, serialize(env.best))["valid"])
        self.assertEqual(env.best_score, [119, 12, 1])
        self.assertEqual(env.best["placements"]["F"], [0, 12, 0])

    @unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build required")
    def test_native_relocation_agrees_with_reference(self):
        runtime = Runtime(ROOT, CASE, self.config, Path(os.environ["AIC_NATIVE_LIBRARY"]))
        native = runtime.new_engine()
        reference = Environment(self.catalog, self.config)
        for command in self.catalog.case["witness"]:
            handle, _ = native.candidates()
            domain = json.loads(native.export_candidates_json(handle))
            reward, _, outcome = native.step(handle, domain.index(command))
            expected, _, accepted = reference.step(command)
            self.assertAlmostEqual(reward, expected)
            self.assertEqual(outcome["applied"], accepted["applied"])
        self.assertEqual(layout(native, "best"), serialize(reference.best))


if __name__ == "__main__":
    unittest.main()
