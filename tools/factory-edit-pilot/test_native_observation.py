"""Native boundary parity/ownership checks, selected with AIC_NATIVE_LIBRARY."""

import json
import os
from pathlib import Path
import unittest

import numpy as np

from acceptance import Catalog
from environment import Environment, Mechanical, SpatialGuard, clone
from native_observation import load_native, native_environment

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent


@unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build path required")
class NativeObservationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.native_type = native_environment(load_native(os.environ["AIC_NATIVE_LIBRARY"]))

    def setUp(self):
        self.catalog = Catalog(ROOT, DIRECTORY/"case.json")
        self.config = json.loads((DIRECTORY/"run.json").read_text())
        self.native = self.native_type(self.catalog, self.config)
        self.python = Environment(self.catalog, self.config)

    def compare(self):
        a, b = self.native.candidates(), self.python.candidates()
        self.assertEqual(a.to_dicts(), b.to_dicts())
        actual, expected = self.native.observe(a), self.python.observe(b)
        for key in expected:
            self.assertEqual(actual[key].dtype, expected[key].dtype, key)
            self.assertEqual(actual[key].shape, expected[key].shape, key)
            self.assertEqual(actual[key].tobytes(), expected[key].tobytes(), key)
        return actual

    def test_witness_every_state_and_output_ownership(self):
        original = self.compare()
        saved = {key: value.copy() for key, value in original.items()}
        for command in self.catalog.case["witness"]:
            self.assertEqual(self.native.step(command), self.python.step(command))
            self.compare()
        for key in saved:
            np.testing.assert_array_equal(original[key], saved[key])
        # A caller may own/mutate a returned array without changing subsequent results.
        original["objects"][:] = 123
        original["raster"][:] = 123
        self.compare()

    def test_rotations_and_unbound_terminals(self):
        for tid in self.python.tids:
            command = {"op": "UNBIND_TERMINALS", "terminal": tid}
            self.assertEqual(self.native.step(command), self.python.step(command))
            self.compare()

        for rotation in (0, 90, 180, 270):
            for env in (self.native, self.python):
                env.draft["placements"][env.fids[0]][2] = rotation
            self.compare()

    def test_bound_port_geometry_under_whole_layout_rotation(self):
        sides = ("N", "E", "S", "W")
        for _ in range(4):
            for env in (self.native, self.python):
                for state in (env.draft, env.base):
                    for fid, pose in state["placements"].items():
                        footprint = self.catalog.facility_type(fid)["footprint"]
                        height = footprint["width" if pose[2] in (90,270) else "height"]
                        pose[:] = [-pose[1]-height+1, pose[0], (pose[2]+90)%360]
                    for net in state["networks"].values():
                        net["cells"] = {(-y,x) for x,y in net["cells"]}
                        net["edges"] = {((-a[1],a[0]),(-b[1],b[0])) for a,b in net["edges"]}
                    for binding in state["bindings"].values():
                        if binding and "cell" in binding:
                            x,y = binding["cell"]
                            binding["cell"] = [-y,x]
                            binding["side"] = sides[(sides.index(binding["side"])+1)%4]
            self.compare()

    def test_negative_and_large_coordinate_channels(self):
        for shift in (-103, 2**24, -(2**24)):
            for env in (self.native, self.python):
                env.reset()
                for state in (env.draft, env.base):
                    for pose in state["placements"].values():
                        pose[0] += shift
                        pose[1] += shift
                    for net in state["networks"].values():
                        net["cells"] = {(x+shift, y+shift) for x,y in net["cells"]}
                        net["edges"] = {((a[0]+shift,a[1]+shift),(b[0]+shift,b[1]+shift)) for a,b in net["edges"]}
                    for binding in state["bindings"].values():
                        if binding and "cell" in binding:
                            binding["cell"] = [x+shift for x in binding["cell"]]
            self.compare()

    def test_stale_candidates_and_resource_guard(self):
        candidates = self.native.candidates()
        self.native.reset()
        with self.assertRaisesRegex(Mechanical, "observation.stale_candidates"):
            self.native.observe(candidates)
        self.native.config = {**self.config, "raster_cell_guard": 1}
        with self.assertRaisesRegex(SpatialGuard, "observation.resource_guard"):
            self.native.observe(self.native.candidates())

    def test_invalid_native_coordinates_and_edges_report_errors(self):
        draft = clone(self.native.draft)
        draft["placements"][self.native.fids[0]][0] = 2**63-1
        with self.assertRaisesRegex(ValueError, "native.input: coordinate overflow"):
            self.native.native_encoder.encode(draft, self.native.base, 2, 16384)
        draft = clone(self.native.draft)
        net = draft["networks"][self.native.nids[0]]
        net["edges"].add(((0,0),(9,9)))
        with self.assertRaisesRegex(ValueError, "native.input:"):
            self.native.native_encoder.encode(draft, self.native.base, 2, 16384)


if __name__ == "__main__":
    unittest.main()
