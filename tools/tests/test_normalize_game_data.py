"""Source-semantic regressions, independent from layout validation against generated catalogs."""
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("normalizer", ROOT / "tools/normalize_game_data.py")
NORMALIZER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NORMALIZER)


def port(x, y, yaw, output=False):
    return {"index": 0, "isOutput": int(output), "isPipe": False,
            "trans": {"position": {"x": x, "y": 0, "z": y},
                      "rotation": {"x": 0, "y": yaw, "z": 0}}}


class FactoryPortDirections(unittest.TestCase):
    def test_corner_position_does_not_override_source_rotation(self):
        self.assertEqual(NORMALIZER.normalized_port(port(4, 4, 180), "input", 5, 5)["edge"], "south")
        self.assertEqual(NORMALIZER.normalized_port(port(4, 4, 270), "input", 5, 5)["edge"], "east")
        self.assertEqual(NORMALIZER.normalized_port(port(0, 0, 90), "input", 5, 5)["edge"], "west")

    def test_all_quarter_turns_and_roles(self):
        for x, y, yaw, expected in [(2, 0, 0, "north"), (0, 2, 90, "west"),
                                    (2, 4, 180, "south"), (4, 2, 270, "east")]:
            for output in [False, True]:
                raw_yaw = (yaw + 180) % 360 if output else yaw
                p = port(x, y, raw_yaw, output)
                self.assertEqual(NORMALIZER.normalized_port(p, "output" if output else "input", 5, 5)["edge"], expected)

    def test_invalid_transforms_roles_and_face_mismatches_fail(self):
        for raw, role in [(port(4, 4, 45), "input"), (port(4, 4, 180), "output"),
                          (port(2, 4, 90), "input"), (port(-1, 4, 180), "input")]:
            with self.assertRaises(ValueError):
                NORMALIZER.normalized_port(raw, role, 5, 5)
        p = port(4, 4, 180)
        p["trans"]["rotation"]["x"] = 90
        with self.assertRaises(ValueError):
            NORMALIZER.normalized_port(p, "input", 5, 5)

    def test_all_planter_inputs_preserve_the_same_source_face(self):
        source = json.loads((ROOT / "data/game/source/table-cfg/FactoryBuildingTable.json").read_text())
        ports = [p for p in source["planter_1"]["inputPorts"] if not p["isPipe"]]
        self.assertEqual(len(ports), 5)
        self.assertEqual({p["trans"]["rotation"]["y"] for p in ports}, {180})
        self.assertEqual({NORMALIZER.normalized_port(p, "input", 5, 5)["edge"] for p in ports}, {"south"})

    def test_all_generated_ports_agree_with_source_noncorner_direction_classes(self):
        source = json.loads((ROOT / "data/game/source/table-cfg/FactoryBuildingTable.json").read_text())
        catalog = json.loads((ROOT / "data/game/normalized/facilities.json").read_text())
        observed = {}
        samples = []
        for f in catalog["facilities"]:
            raw = next(b for b in source.values() if b["id"].replace("_", "-") == f["id"].split("-mode-")[0])
            for p in f["ports"]:
                original = next(q for q in raw["inputPorts" if p["direction"] == "input" else "outputPorts"]
                                if q["index"] == int(p["id"].rsplit("-", 1)[1]) and q["isPipe"] == (p["transport"] == "pipe"))
                x, y = original["trans"]["position"]["x"], original["trans"]["position"]["z"]
                faces = [face for face, condition in [("north", y == 0), ("east", x == raw["range"]["width"] - 1),
                         ("south", y == raw["range"]["depth"] - 1), ("west", x == 0)] if condition]
                key = (p["direction"], original["trans"]["rotation"]["y"])
                if len(faces) == 1:
                    observed.setdefault(key, set()).add(faces[0])
                samples.append((p, key, original))
        self.assertEqual(len(samples), 250)
        for p, key, original in samples:
            self.assertEqual(len(observed[key]), 1)
            self.assertIn(p["edge"], observed[key], p["id"])
            self.assertEqual(p["position"], {"x": original["trans"]["position"]["x"], "y": original["trans"]["position"]["z"]})


if __name__ == "__main__":
    unittest.main()
