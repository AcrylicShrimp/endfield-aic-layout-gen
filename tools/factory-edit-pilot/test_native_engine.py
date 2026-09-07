"""Native state admission and transactional snapshot parity against the migration oracle."""

from copy import deepcopy
import json
import os
import random
from pathlib import Path
import tempfile
import unittest

from acceptance import Catalog, check
from environment import Environment, serialize
from native_observation import load_native

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = Path(__file__).resolve().parent
CONFIG_KEYS = ("horizon", "halo", "raster_cell_guard", "move_lengths", "segment_lengths")


@unittest.skipUnless(os.environ.get("AIC_NATIVE_LIBRARY"), "explicit native build path required")
class NativeEngineTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = load_native(os.environ["AIC_NATIVE_LIBRARY"])

    def setUp(self):
        self.catalog = Catalog(ROOT, DIRECTORY / "case.json")
        self.config = json.loads((DIRECTORY / "run.json").read_text())
        self.config_json = json.dumps({key: self.config[key] for key in CONFIG_KEYS})
        self.engine = self.open()
        self.reference = Environment(self.catalog, self.config)

    def open(self, case_path=None):
        return self.module.Engine.open(str(ROOT), str(case_path or DIRECTORY / "case.json"), self.config_json)

    def layout(self, which="draft"):
        return json.loads(self.engine.export_layout_json(which))

    def snapshot(self):
        return json.loads(self.engine.export_state_json())

    def restore(self, snapshot):
        self.engine.restore_state_json(json.dumps(snapshot))

    def domain(self):
        expected = self.reference.candidates().to_dicts()
        handle, count = self.engine.candidates()
        actual = json.loads(self.engine.export_candidates_json(handle))
        self.assertEqual(count, len(expected))
        self.assertEqual(actual, expected)
        return handle, actual

    def compare_state(self):
        snapshot = self.snapshot()
        for which in ("draft", "base", "best"):
            self.assertEqual(snapshot[which], serialize(getattr(self.reference, which)), which)
        for field in ("base_score", "best_score", "steps", "revision", "trace"):
            self.assertEqual(snapshot[field], getattr(self.reference, field), field)
        self.assertEqual(snapshot["episode_counts"], dict(self.reference.episode_counts))

    def execute(self, command, *, diagnostic=False):
        handle, commands = self.domain()
        if diagnostic:
            result = self.engine.diagnostic_step_json(json.dumps(command))
        else:
            result = self.engine.step(handle, commands.index(command))
        self.assertEqual(result, self.reference.step(command))
        self.compare_state()
        return result

    def test_witness_relocation_reward_and_discard_transitions(self):
        relocation = [
            {"op": "UNBIND_TERMINALS", "terminal": "glass:source"},
            {"op": "UNBIND_TERMINALS", "terminal": "quartz:target"},
            {"op": "ERASE_TRANSPORT", "network": "glass", "cells": [[1, 13], [1, 12]]},
            {"op": "SET_PLACEMENTS", "assignments": {"F": [0, 12, 0]}},
            {"op": "BIND_TERMINALS", "terminal": "glass:source", "binding": {"port": "output-belt-1"}},
            {"op": "EXTEND", "network": "quartz", "anchor": [1, 17], "growth": "forward", "direction": "N", "length": 2},
            {"op": "BIND_TERMINALS", "terminal": "quartz:target", "binding": {"port": "input-belt-1"}},
        ]
        for command in relocation + self.catalog.case["witness"]:
            self.execute(command)
        self.assertLess(self.snapshot()["best_score"], self.reference.initial_score)
        for command in [{"op": "UNBIND_TERMINALS", "terminal": "quartz:source"}, {"op": "DISCARD_EDIT"}]:
            self.execute(command)
        self.engine.reset()
        self.reference.reset()
        self.compare_state()

    def test_random_domain_and_every_transition(self):
        rng = random.Random(99)
        seen = set()
        for _ in range(8):
            for _ in range(self.config["horizon"]):
                handle, commands = self.domain()
                selected = rng.randrange(len(commands))
                command = commands[selected]
                seen.add(command["op"])
                self.assertEqual(self.engine.step(handle, selected), self.reference.step(command))
                self.compare_state()
            with self.assertRaisesRegex(ValueError, "engine.ended"):
                self.engine.diagnostic_step_json(json.dumps({"op": "DISCARD_EDIT"}))
            self.engine.reset()
            self.reference.reset()
        self.assertEqual(seen, set(json.loads(self.engine.metadata_json())["operations"]))

    def test_rejected_edits_are_charged_and_api_errors_are_not(self):
        commands = [
            {"op": "SET_PLACEMENTS", "assignments": {"F": [0, 0, 0]}},
            {"op": "UNBIND_TERMINALS", "terminal": "quartz:source", "revision": 0},
            {"op": "ERASE_TRANSPORT", "network": "quartz", "cells": [[1, 19]]},
            {"op": "CONNECT", "network": "quartz", "source": [1, 19], "target": [1, 17]},
            {"op": "EXTEND", "network": "quartz", "anchor": [1, 19], "growth": "forward", "direction": "N", "length": 2},
            {"op": "BIND_TERMINALS", "terminal": "quartz:source", "binding": {"cell": [1, 19], "side": "S"}},
        ]
        for command in commands:
            self.assertFalse(self.execute(command, diagnostic=True)[2]["applied"])
        handle, commands = self.domain()
        original = self.snapshot()
        with self.assertRaisesRegex(ValueError, "engine.action_index"):
            self.engine.step(handle, len(commands))
        self.assertEqual(self.snapshot(), original)
        other = self.open()
        other_handle, _ = other.candidates()
        with self.assertRaisesRegex(ValueError, "engine.stale_handle"):
            self.engine.step(other_handle, 0)
        self.assertEqual(self.snapshot(), original)
        self.restore(original)
        after_restore = self.snapshot()
        with self.assertRaisesRegex(ValueError, "engine.stale_handle"):
            self.engine.step(handle, 0)
        self.assertEqual(self.snapshot(), after_restore)
        handle, _ = self.domain()
        self.engine.reset()
        reset = self.snapshot()
        with self.assertRaisesRegex(ValueError, "engine.stale_handle"):
            self.engine.step(handle, 0)
        self.assertEqual(self.snapshot(), reset)

    def test_partial_episode_restore_continues_exactly(self):
        for command in self.catalog.case["witness"][:3]:
            self.execute(command)
        snapshot = self.snapshot()
        restored = self.open()
        restored.restore_state_json(json.dumps(snapshot))
        old_handle, _ = self.engine.candidates()
        with self.assertRaisesRegex(ValueError, "engine.stale_handle"):
            restored.step(old_handle, 0)
        self.engine = restored
        for command in self.catalog.case["witness"][3:]:
            self.execute(command)

    def test_identifier_escaping_preserves_canonical_order(self):
        case = deepcopy(self.catalog.case)
        fids = {f["id"]: f["id"] + "é\"\\\x7f😀" for f in case["facilities"]}
        for facility in case["facilities"]:
            facility["id"] = fids[facility["id"]]
        for i, network in enumerate(case["networks"]):
            network["id"] += "\n" + str(10 - i) + "é😀"
            for role in ("source", "target"):
                endpoint = network[role]
                if "facility" in endpoint:
                    endpoint["facility"] = fids[endpoint["facility"]]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "case.json"
            path.write_text(json.dumps(case))
            self.catalog = Catalog(ROOT, path)
            self.reference = Environment(self.catalog, self.config)
            self.engine = self.open(path)
            for tid in self.reference.tids:
                self.execute({"op": "UNBIND_TERMINALS", "terminal": tid})
            self.domain()

    def test_admission_export_and_owned_snapshot(self):
        expected = serialize(self.reference.root)
        for which in ("root", "draft", "base", "best"):
            self.assertEqual(self.layout(which), expected)
        metadata = json.loads(self.engine.metadata_json())
        self.assertEqual(metadata["initial_score"], self.reference.initial_score)
        self.assertEqual(metadata["facility_ids"], self.reference.fids)
        self.assertEqual(metadata["network_ids"], self.reference.nids)
        self.assertEqual(metadata["terminal_ids"], self.reference.tids)
        self.assertEqual(metadata["case"], self.catalog.case)
        saved = self.snapshot()
        self.restore(saved)
        after = self.snapshot()
        self.assertGreater(after.pop("generation"), saved.pop("generation"))
        self.assertEqual(after, saved)
        saved["draft"]["placements"]["F"][0] = 100
        self.assertEqual(self.layout(), expected)

    def test_partial_draft_restore_reset_and_cumulative_counters(self):
        self.reference.step(self.catalog.case["witness"][0])
        snapshot = self.snapshot()
        snapshot.update(draft=serialize(self.reference.draft), base=serialize(self.reference.base),
                        best=serialize(self.reference.best), base_score=self.reference.base_score,
                        best_score=self.reference.best_score, steps=self.reference.steps,
                        revision=self.reference.revision, trace=self.reference.trace,
                        episode_counts=dict(self.reference.episode_counts), counters={"example": 3},
                        seconds={"example": 0.25})
        self.restore(snapshot)
        self.assertEqual(self.layout(), serialize(self.reference.draft))
        self.assertIsNone(self.layout()["bindings"]["quartz:source"])
        self.engine.reset()
        self.assertEqual(self.layout(), serialize(self.reference.root))
        reset = self.snapshot()
        self.assertEqual(reset["steps"], 0)
        self.assertEqual(reset["trace"], [])
        self.assertEqual(reset["episode_counts"], {})
        self.assertEqual(reset["counters"], {"example": 3})
        self.assertEqual(reset["seconds"], {"example": 0.25})

    def test_acceptance_witness_and_individual_invariant_failures(self):
        for command in [None, *self.catalog.case["witness"]]:
            if command:
                self.reference.step(command)
            layout = serialize(self.reference.draft)
            self.assertEqual(json.loads(self.engine.check_layout_json(json.dumps(layout))),
                             check(self.catalog, layout))
        original = self.layout()
        mutations = [
            lambda x: x.update(schema_version=2),
            lambda x: x["placements"].pop("F"),
            lambda x: x["networks"].pop("glass"),
            lambda x: x["bindings"].pop("glass:source"),
            lambda x: x["placements"]["F"].__setitem__(2, 45),
            lambda x: x["placements"].__setitem__("G", [0, 8, 0]),
            lambda x: x["networks"]["quartz"]["cells"].append([1, 19]),
            lambda x: x["networks"]["quartz"]["edges"].append([[1, 19], [1, 18]]),
            lambda x: x["networks"]["quartz"]["edges"].clear(),
            lambda x: x["bindings"].__setitem__("quartz:source", None),
            lambda x: x["bindings"].__setitem__("quartz:source", {"cell": [1, 18], "side": "S"}),
            lambda x: x["bindings"].__setitem__("glass:source", {"port": "missing"}),
            lambda x: x["bindings"].__setitem__("glass:source", {"port": "input-belt-1"}),
        ]
        for mutate in mutations:
            layout = deepcopy(original)
            mutate(layout)
            expected = check(self.catalog, layout)
            actual = json.loads(self.engine.check_layout_json(json.dumps(layout)))
            self.assertFalse(expected["valid"])
            # Independent validators may encounter different invalid networks first.
            self.assertEqual(actual["valid"], expected["valid"])
            self.assertEqual(actual["diagnostics"][0]["code"], expected["diagnostics"][0]["code"])

    def test_bound_rotations_and_translation(self):
        layout = self.layout()
        sides = ("N", "E", "S", "W")
        for _ in range(4):
            for fid, pose in layout["placements"].items():
                definition = self.catalog.facility_type(fid)["footprint"]
                height = definition["width" if pose[2] in (90, 270) else "height"]
                pose[:] = [-pose[1] - height + 1, pose[0], (pose[2] + 90) % 360]
            for network in layout["networks"].values():
                network["cells"] = [[-y, x] for x, y in network["cells"]]
                network["edges"] = [[[-a[1], a[0]], [-b[1], b[0]]] for a, b in network["edges"]]
            for binding in layout["bindings"].values():
                if "cell" in binding:
                    x, y = binding["cell"]
                    binding["cell"] = [-y, x]
                    binding["side"] = sides[(sides.index(binding["side"]) + 1) % 4]
            expected = check(self.catalog, layout)
            self.assertTrue(expected["valid"])
            self.assertEqual(json.loads(self.engine.check_layout_json(json.dumps(layout))), expected)
        for shift in (-103, 2**24, -(2**24)):
            shifted = deepcopy(layout)
            for pose in shifted["placements"].values():
                pose[:2] = [v + shift for v in pose[:2]]
            for network in shifted["networks"].values():
                network["cells"] = [[x + shift, y + shift] for x, y in network["cells"]]
                network["edges"] = [[[x + shift, y + shift] for x, y in edge] for edge in network["edges"]]
            for binding in shifted["bindings"].values():
                if "cell" in binding:
                    binding["cell"] = [v + shift for v in binding["cell"]]
            self.assertEqual(json.loads(self.engine.check_layout_json(json.dumps(shifted))), check(self.catalog, shifted))

    def test_failed_restore_is_atomic_and_does_not_trust_scores(self):
        original = self.snapshot()
        mutations = [
            ("snapshot.version", lambda s: s.update(schema_version=1)),
            ("snapshot.provenance", lambda s: s["provenance"].update(case="wrong")),
            ("snapshot.provenance", lambda s: s["config"].update(horizon=1)),
            ("snapshot.accounting", lambda s: s.update(steps=1)),
            ("snapshot.score", lambda s: s.update(best_score=[1, 0, 0])),
            ("terminal.unbound", lambda s: s["base"]["bindings"].__setitem__("quartz:source", None)),
            ("edit.rejected", lambda s: s["draft"]["networks"]["quartz"]["cells"].remove([1, 19])),
        ]
        for code, mutate in mutations:
            snapshot = deepcopy(original)
            mutate(snapshot)
            with self.assertRaisesRegex(ValueError, code):
                self.restore(snapshot)
            self.assertEqual(self.snapshot(), original)
        with self.assertRaisesRegex(ValueError, "snapshot.version"):
            self.restore({"draft": original["draft"]})

    def test_exact_rates_and_path_diagnostics(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "case.json"
            case = deepcopy(self.catalog.case)
            # Equivalent large rationals must load without overflowing intermediate products.
            case["facilities"][0]["utilization"] = [2**60, 2**61]
            case["networks"][0]["rate"] = [2**60, 2**62]
            path.write_text(json.dumps(case))
            self.assertEqual(json.loads(self.open(path).export_layout_json("root")), self.layout("root"))
            # The perturbation disappears in float64, but changes the exact obligation.
            case["facilities"][0]["utilization"][0] += 1
            path.write_text(json.dumps(case))
            with self.assertRaisesRegex(ValueError, "request.coverage"):
                self.open(path)
            case["facilities"][0]["utilization"][1] = 0
            path.write_text(json.dumps(case))
            with self.assertRaisesRegex(ValueError, "input.rational"):
                self.open(path)
            missing = Path(directory) / "missing.json"
            with self.assertRaisesRegex(ValueError, "input.read.*missing.json"):
                self.open(missing)

    def test_integer_and_geometry_range_rejection(self):
        for value in (True, 1.5, 2**63):
            layout = self.layout()
            layout["placements"]["F"][0] = value
            actual = json.loads(self.engine.check_layout_json(json.dumps(layout)))
            self.assertFalse(actual["valid"])
        layout = self.layout()
        layout["placements"]["F"][0] = 2**63 - 1
        actual = json.loads(self.engine.check_layout_json(json.dumps(layout)))
        self.assertEqual(actual["diagnostics"][0]["code"], "geometry.range")


if __name__ == "__main__":
    unittest.main()
