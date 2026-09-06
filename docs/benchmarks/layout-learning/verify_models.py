"""Reload every JSON checkpoint and reproduce every recorded held-out selection."""

import argparse
import json
import sys
from pathlib import Path

import torch
from model import pack, restore
from train import predict, read, sha, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--training-directory", type=Path, required=True)
    parser.add_argument("--report-output", type=Path, required=True)
    options = parser.parse_args()
    report = read(options.training_directory / "training.json")
    torch.set_num_threads(report["threads"])
    dataset = Path(report["dataset_directory"]) / "dataset.json"
    if sha(dataset) != report["dataset_sha256"]:
        raise ValueError("Dataset hash changed")
    samples = read(dataset)["samples"]
    checks = []
    for run in report["runs"]:
        checkpoint_path = options.training_directory / run["checkpoint"]["file"]
        if sha(checkpoint_path) != run["checkpoint"]["sha256"]:
            raise ValueError(f"Checkpoint hash changed: {checkpoint_path}")
        checkpoint = read(checkpoint_path)
        model = restore(checkpoint, report["device"]).eval()
        for split, choices in run["choices"].items():
            rows = [s for s in samples if s["split"] == split]
            predicted = predict(model, pack(rows, checkpoint["normalization"], report["device"]))
            expected = [{"sample": s["id"], "action": i} for s, i in zip(rows, predicted)]
            if choices != expected:
                raise ValueError(f"Reloaded predictions differ: {run['name']} / {split}")
            checks.append({"run": run["name"], "split": split, "identical_predictions": len(rows)})
    result = {"schema_version": 1, "status": "complete", "training_sha256": sha(options.training_directory / "training.json"), "checks": checks}
    write(options.report_output, result)
    print(json.dumps({"status": "complete", "identical_predictions": sum(c["identical_predictions"] for c in checks)}))


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(json.dumps({"code": "learning-model-verification-failed", "severity": "error", "stage": "model-verification", "message": str(e)}), file=sys.stderr)
        sys.exit(1)
