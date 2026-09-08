"""Validated geometric augmentation of a supplied cardinal relocation witness."""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import tempfile

from curriculum import prefixes
from native import Runtime, layout
from reference.acceptance import Catalog, check, footprint
from reference.environment import Environment, serialize


def require(condition, code, message):
    if not condition:
        raise ValueError(f"augmentation.{code}: {message}")


def rotate(cell, turns):
    x, y = cell
    for _ in range(turns):
        x, y = -y, x
    return [x, y]


def transform(catalog, gap, turns):
    require(type(gap) is int and gap >= 0 and type(turns) is int and 0 <= turns < 4,
            "parameters", "gap must be nonnegative and quarter turns must be in 0..3")
    case = copy.deepcopy(catalog.case)
    moves = [c for c in case["witness"] if c["op"] == "SET_PLACEMENTS"]
    require(len(moves) == 1 and len(moves[0]["assignments"]) == 1,
            "witness", "exactly one single-facility placement edit required")
    fid, destination = next(iter(moves[0]["assignments"].items()))
    origin = catalog.facilities[fid]["pose"]
    dx, dy = destination[0] - origin[0], destination[1] - origin[1]
    require((dx == 0) != (dy == 0) and origin[2] == destination[2],
            "witness", "placement edit must be a cardinal translation")
    delta = [-gap * (dx // abs(dx) if dx else 0), -gap * (dy // abs(dy) if dy else 0)]

    def shift(cell):
        return [cell[0] + delta[0], cell[1] + delta[1]]

    incident = set()
    for facility in case["facilities"]:
        if facility["id"] == fid:
            facility["pose"][:2] = shift(facility["pose"][:2])
    for network in case["networks"]:
        roles = [role for role in ("source", "target") if network[role].get("facility") == fid]
        if not roles:
            continue
        require(len(roles) == 1, "corridor", f"self loop unsupported: {network['id']}")
        incident.add(network["id"])
        role = roles[0]
        other = "target" if role == "source" else "source"
        if "external" in network[other]:
            network["corners"] = [shift(cell) for cell in network["corners"]]
            network[other + "_binding"]["cell"] = shift(network[other + "_binding"]["cell"])
        else:
            corners = network["corners"]
            require(len(corners) == 2 and ((dx == 0 and corners[0][0] == corners[1][0])
                                          or (dy == 0 and corners[0][1] == corners[1][1])),
                    "corridor", f"straight parallel internal corridor required: {network['id']}")
            endpoint = 0 if role == "source" else -1
            corners[endpoint] = shift(corners[endpoint])
    for command in case["witness"]:
        op = command["op"]
        require(op in {"SET_PLACEMENTS", "ERASE_TRANSPORT", "BIND_TERMINALS", "UNBIND_TERMINALS"},
                "witness", f"unsupported witness operation: {op}")
        if op == "SET_PLACEMENTS":
            command["assignments"][fid][:2] = shift(command["assignments"][fid][:2])
        elif op == "ERASE_TRANSPORT" and command["network"] in incident:
            command["cells"] = [shift(cell) for cell in command["cells"]]
        elif op == "BIND_TERMINALS" and command["terminal"].split(":")[0] in incident:
            if "cell" in command["binding"]:
                command["binding"]["cell"] = shift(command["binding"]["cell"])

    def pose(facility_id, value):
        cells = [rotate(cell, turns) for cell in footprint(catalog, facility_id, value)]
        result = [min(c[0] for c in cells), min(c[1] for c in cells), (value[2] + 90 * turns) % 360]
        footprint(catalog, facility_id, result)  # Check catalog rotation permission.
        return result

    def binding(value):
        if "cell" in value:
            value["cell"] = rotate(value["cell"], turns)
            sides = "NESW"
            value["side"] = sides[(sides.index(value["side"]) + turns) % 4]

    for facility in case["facilities"]:
        facility["pose"] = pose(facility["id"], facility["pose"])
    for network in case["networks"]:
        network["corners"] = [rotate(cell, turns) for cell in network["corners"]]
        for role in ("source", "target"):
            binding(network[role + "_binding"])
    for command in case["witness"]:
        if command["op"] == "SET_PLACEMENTS":
            command["assignments"] = {key: pose(key, value) for key, value in command["assignments"].items()}
        elif "cells" in command:
            command["cells"] = [rotate(cell, turns) for cell in command["cells"]]
        elif "binding" in command:
            binding(command["binding"])
    return case


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def validate(root, path, config, library):
    catalog = Catalog(root, path)
    reference = Environment(catalog, config)
    runtime = Runtime(root, path, config, library)
    native = runtime.new_engine()
    require(check(catalog, serialize(reference.best))["valid"], "initial", str(path))
    first_reward = None
    for index, command in enumerate(catalog.case["witness"], 1):
        require(command in reference.candidates().to_dicts(), "reference_domain", f"{path}: step {index}")
        handle, _ = native.candidates()
        domain = json.loads(native.export_candidates_json(handle))
        require(command in domain, "native_domain", f"{path}: step {index}")
        actual = native.step(handle, domain.index(command))
        expected = reference.step(command)
        require(actual[2]["applied"] and expected[2]["applied"] and not actual[1] and not expected[1],
                "transition", f"{path}: step {index}")
        require(abs(actual[0] - expected[0]) < 1e-6 and layout(native, "best") == serialize(reference.best),
                "parity", f"{path}: step {index}")
        require(check(catalog, serialize(reference.best))["valid"], "acceptance", f"{path}: step {index}")
        if actual[0] > 0 and first_reward is None:
            first_reward = index
    require(reference.best_score[0] < reference.initial_score[0], "gain", str(path))
    require(any(reference.best["placements"][fid][:2] != f["pose"][:2]
                for fid, f in catalog.facilities.items()), "translation", str(path))
    starts = prefixes(runtime)
    return {"initial_score": reference.initial_score, "witness_score": reference.best_score,
            "first_reward_distance": first_reward, "curriculum_levels": len(starts) - 1}


def generate(root, spec_path, output, library):
    spec = json.loads(spec_path.read_text())
    require(spec["schema_version"] == 1, "schema", str(spec_path))
    require(set(spec["splits"]) == {"train", "development", "heldout"}, "splits", str(spec_path))
    require(not output.exists(), "output_exists", str(output))
    catalog = Catalog(root, root / spec["base_case"])
    config = json.loads((root / spec["config"]).read_text())
    config.update(device="cpu", horizon=max(config["horizon"], len(catalog.case["witness"]) + 1))
    rows, cases, seen = [], {}, set()
    with tempfile.TemporaryDirectory(prefix="aic-augmentation-") as directory:
        for split, values in spec["splits"].items():
            require(values["gap_extensions"] and values["quarter_turns"], "splits", f"empty split: {split}")
            for gap in values["gap_extensions"]:
                for turns in values["quarter_turns"]:
                    case = transform(catalog, gap, turns)
                    identity = digest({key: value for key, value in case.items() if key not in {"id", "witness"}})
                    require(identity not in seen, "duplicate", f"{split}: gap={gap}, turns={turns}")
                    seen.add(identity)
                    key = f"{split}-gap{gap}-turn{turns}"
                    case["id"] = key
                    path = Path(directory) / f"{key}.json"
                    path.write_text(json.dumps(case, indent=2) + "\n")
                    evidence = validate(root, path, config, library)
                    cases[path.name] = path.read_bytes()
                    rows.append({"id": key, "path": path.name, "split": split,
                                 "gap_extension": gap, "quarter_turns": turns,
                                 "geometry_sha256": identity,
                                 "case_sha256": hashlib.sha256(cases[path.name]).hexdigest(), **evidence})
    inputs = [spec_path, root / spec["base_case"], root / spec["config"], library, *catalog.paths.values()]
    manifest = {"schema_version": 1, "stage": "augmentation", "status": "passed",
                "inputs_sha256": {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs},
                "cases": rows}
    output.mkdir(parents=True)
    for name, content in cases.items():
        (output / name).write_bytes(content)
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = generate(Path(__file__).resolve().parents[2], args.spec.resolve(),
                          args.output.resolve(), args.native_library.resolve())
    except (ValueError, KeyError, OSError) as error:
        print(json.dumps({"code": str(error).split(":")[0] if str(error).startswith("augmentation.")
                          else "augmentation.input", "severity": "error", "stage": "augmentation",
                          "message": str(error), "entity": str(args.spec)}))
        raise SystemExit(1) from error
    print(json.dumps({"status": result["status"], "cases": len(result["cases"]), "output": str(args.output)}))


if __name__ == "__main__":
    main()
