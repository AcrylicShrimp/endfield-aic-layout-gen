"""Compare recorded training states and outcomes at identical command exposure."""

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import torch

from evidence import write


FIELDS = ("policy", "optimizer", "torch_rng", "numpy_rng", "numpy_legacy_rng", "python_rng",
          "cuda_rng", "commands", "episode", "optimizer_steps", "counts", "champion_score")


def equal(a, b):
    if isinstance(a, torch.Tensor):
        return isinstance(b, torch.Tensor) and a.dtype == b.dtype and a.shape == b.shape and torch.equal(a, b)
    if isinstance(a, np.ndarray):
        return isinstance(b, np.ndarray) and a.dtype == b.dtype and np.array_equal(a, b)
    if isinstance(a, dict):
        return isinstance(b, dict) and a.keys() == b.keys() and all(equal(a[key], b[key]) for key in a)
    if isinstance(a, (tuple, list)):
        return isinstance(b, type(a)) and len(a) == len(b) and all(equal(x, y) for x, y in zip(a, b))
    return a == b


def policy_hash(state):
    digest = hashlib.sha256()
    for key, value in sorted(state.items()):
        digest.update(key.encode())
        digest.update(str(value.dtype).encode())
        digest.update(str(tuple(value.shape)).encode())
        digest.update(value.detach().cpu().contiguous().numpy().tobytes())
    return digest.hexdigest()


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def compare(reference, native):
    metadata = [json.loads((path/"metadata.json").read_text()) for path in (reference, native)]
    if any(value["status"] != "complete" for value in metadata):
        raise ValueError("comparison.incomplete: both runs must have completed")
    if metadata[0]["config"] != metadata[1]["config"]:
        raise ValueError("comparison.exposure: configurations differ")
    if any(value["completed_commands"] != value["config"]["commands"] for value in metadata):
        raise ValueError("comparison.exposure: completed command count differs from the allowance")
    if metadata[0]["case"] != metadata[1]["case"] or metadata[0]["verification"] != metadata[1]["verification"]:
        raise ValueError("comparison.inputs: case or initial/witness acceptance differs")
    for field in ("python", "torch", "numpy", "platform", "machine"):
        if metadata[0]["provenance"][field] != metadata[1]["provenance"][field]:
            raise ValueError(f"comparison.runtime: {field} differs")
    native_inputs = set(metadata[1]["provenance"]["native"]["inputs"].values())
    reference_inputs = set(metadata[0]["provenance"]["files_sha256"].values())
    reference_inputs.update(metadata[0]["provenance"].get("external_files_sha256", {}).values())
    if not native_inputs <= reference_inputs:
        raise ValueError("comparison.inputs: native input hashes are absent from the reference provenance")
    commands = metadata[0]["config"]["checkpoints"]
    checkpoints = []
    for count in commands:
        paths = [path/f"checkpoint-{count}.pt" for path in (reference, native)]
        a, b = [torch.load(path, weights_only=False, map_location="cpu") for path in paths]
        fields = {key: equal(a[key], b[key]) for key in FIELDS}
        checkpoints.append({"commands": count, "fields_equal": fields,
                            "policy_sha256": {"reference": policy_hash(a["policy"]), "native": policy_hash(b["policy"])},
                            "checkpoint_sha256": {kind: hashlib.sha256(path.read_bytes()).hexdigest()
                                                 for kind, path in zip(("reference", "native"), paths)}})
    comparisons = {}
    for name in ("episodes.jsonl", "evaluations.jsonl"):
        a, b = [rows(path/name) for path in (reference, native)]
        first_difference = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), None)
        if first_difference is None and len(a) != len(b):
            first_difference = min(len(a), len(b))
        comparisons[name] = {"equal": a == b, "reference_rows": len(a), "native_rows": len(b),
                             "first_different_row": first_difference}
    return {"schema_version": 1, "reference": str(reference.resolve()), "native": str(native.resolve()),
            "equal_exposure": True, "commands": metadata[0]["completed_commands"],
            "compared_checkpoint_fields": list(FIELDS), "checkpoints": checkpoints, "rows": comparisons,
            "all_recorded_states_equal": all(all(row["fields_equal"].values()) for row in checkpoints)
                                         and all(row["equal"] for row in comparisons.values()),
            "training_seconds": {kind: value["training_seconds"] for kind, value in zip(("reference", "native"), metadata)},
            "metadata_sha256": {kind: hashlib.sha256((path/"metadata.json").read_bytes()).hexdigest()
                                for kind, path in zip(("reference", "native"), (reference, native))},
            "comparison_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f"output must be fresh: {args.output}")
    result = compare(args.reference, args.native)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    write(args.output, result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
