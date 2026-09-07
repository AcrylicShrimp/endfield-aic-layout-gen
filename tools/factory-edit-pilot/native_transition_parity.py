"""Replay actions and observation bytes through the Rust engine and Python migration oracle."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import torch

from reference.acceptance import Catalog
from reference.environment import Environment, serialize
from native import Runtime


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
    config = json.loads(args.config.read_text())
    runtime = Runtime(args.root, args.case, config, args.native_library)
    catalog = Catalog(args.root, args.case)
    results = []
    for path in args.input:
        rollout = torch.load(path, weights_only=False, map_location="cpu")["rollout"]
        native = runtime.new_engine()
        reference = Environment(catalog, config)
        operations = Counter()
        for step, selected in enumerate(rollout["actions"]):
            reference_candidates = reference.candidates()
            expected = reference_candidates.to_dicts()
            handle, count = native.candidates()
            actual = json.loads(native.export_candidates_json(handle))
            if count != len(expected) or actual != expected:
                raise ValueError(f"native.parity: {path} step {step} candidate domain/order differs")
            observation = native.observe_candidates(handle)
            reference_observation = reference.observe(reference_candidates)
            for key, actual_array in observation.items():
                for expected_array in (reference_observation[key], rollout["observations"][step][key]):
                    if (actual_array.dtype != expected_array.dtype or actual_array.shape != expected_array.shape
                            or actual_array.tobytes() != expected_array.tobytes()):
                        raise ValueError(f"native.parity: {path} step {step} {key} observation bytes differ")
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
            for key in ("candidate_commands", "sort_cache_hits", "sort_cache_misses", "sort_cache_entries",
                        "encoding_cache_hits", "encoding_cache_misses", "encoding_cache_entries"):
                if snapshot["counters"].get(key, 0) != reference.counters[key]:
                    raise ValueError(f"native.parity: {path} step {step} {key} differs")
            if result[1]:
                native.reset()
                reference.reset()
        row = {"input": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
               "commands": len(rollout["actions"]), "operations": dict(operations), "parity": "bit_exact"}
        results.append(row)
        print(json.dumps(row), flush=True)
    sources = [*sorted((args.root / "crates/aic-pilot-native/src").glob("*.rs")),
               Path(__file__), args.root / "Cargo.lock", args.root / "Cargo.toml",
               args.root / "crates/aic-pilot-native/Cargo.toml", args.root / "crates/aic-pilot-native/build.rs",
               args.config, args.native_library]
    report = {"schema_version": 2, "parity": "bit_exact", "rows": results,
              "native_provenance": runtime.provenance,
              "metadata": json.loads(native.metadata_json()),
              "sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in sources}}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
