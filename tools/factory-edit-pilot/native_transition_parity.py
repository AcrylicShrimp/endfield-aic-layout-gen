"""Replay retained actions through the owned Rust engine and independent Python oracle."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import torch

from acceptance import Catalog
from environment import Environment, serialize
from native_observation import load_native


CONFIG_KEYS = ("horizon", "halo", "raster_cell_guard", "move_lengths", "segment_lengths")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f"output must be fresh: {args.output}")
    module = load_native(args.native_library)
    config = json.loads(args.config.read_text())
    config_json = json.dumps({key: config[key] for key in CONFIG_KEYS})
    catalog = Catalog(args.root, args.case)
    results = []
    for path in args.input:
        rollout = torch.load(path, weights_only=False, map_location="cpu")["rollout"]
        native = module.Engine.open(str(args.root), str(args.case), config_json)
        reference = Environment(catalog, config)
        operations = Counter()
        for step, selected in enumerate(rollout["actions"]):
            expected = reference.candidates().to_dicts()
            handle, count = native.candidates()
            actual = json.loads(native.export_candidates_json(handle))
            if count != len(expected) or actual != expected:
                raise ValueError(f"native.parity: {path} step {step} candidate domain/order differs")
            command = expected[selected]
            operations[command["op"]] += 1
            result = native.step(handle, selected)
            if result != reference.step(command):
                raise ValueError(f"native.parity: {path} step {step} transition differs")
            snapshot = json.loads(native.export_state_json())
            for field in ("draft", "base", "best"):
                if snapshot[field] != serialize(getattr(reference, field)):
                    raise ValueError(f"native.parity: {path} step {step} {field} differs")
            for field in ("base_score", "best_score", "steps", "revision", "trace", "episode_counts"):
                if snapshot[field] != getattr(reference, field):
                    raise ValueError(f"native.parity: {path} step {step} {field} differs")
            for key in ("candidate_commands", "sort_cache_hits", "sort_cache_misses", "sort_cache_entries"):
                if snapshot["counters"].get(key, 0) != reference.counters[key]:
                    raise ValueError(f"native.parity: {path} step {step} {key} differs")
            if result[1]:
                native.reset()
                reference.reset()
        row = {"input": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
               "commands": len(rollout["actions"]), "operations": dict(operations), "parity": "exact"}
        results.append(row)
        print(json.dumps(row), flush=True)
    sources = [*sorted((args.root / "crates/aic-pilot-native/src").glob("*.rs")),
               Path(__file__), args.root / "Cargo.lock", args.config, args.native_library]
    report = {"schema_version": 1, "parity": "exact", "rows": results,
              "metadata": json.loads(native.metadata_json()),
              "sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
