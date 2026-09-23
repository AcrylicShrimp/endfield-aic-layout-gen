"""Select a fixed diagnostic panel disjoint from the eight training panels."""

import argparse
import hashlib
import json
from pathlib import Path


def identity(case):
    encoded = json.dumps(case["problem"], sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(encoded.encode()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidates", type=Path, required=True)
    parser.add_argument("--training-panels", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    training_paths = sorted(args.training_panels.glob("*.json"))
    if len(training_paths) != 8:
        raise ValueError(f"expected eight training panels, found {len(training_paths)}")
    training = {
        identity(case)
        for path in training_paths
        for case in json.loads(path.read_text())["cases"]
    }
    panel = json.loads(args.candidates.read_text())
    if panel["partition"] != "training" or len(panel["cases"]) != 256:
        raise ValueError("expected 256 train-source diagnostic candidates")

    selected = []
    seen = set()
    for case in panel["cases"]:
        key = identity(case)
        if key not in training and key not in seen:
            selected.append(case)
            seen.add(key)
        if len(selected) == 64:
            break
    if len(selected) != 64:
        raise ValueError(f"only {len(selected)} disjoint diagnostic cases")

    panel["cases"] = selected
    args.output.write_text(json.dumps(panel, indent=2) + "\n")
    print(f"selected={len(selected)} training_identities={len(training)} "
          f"sha256={hashlib.sha256(args.output.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
