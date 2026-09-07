"""Audit retained layouts/traces against original source hashes and exact acceptance."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from acceptance import Catalog, check
from environment import Environment, serialize


def audit(root, case, output, source_commit):
    catalog = Catalog(root, case)
    metadata = json.loads((output/"metadata.json").read_text())
    config = metadata["config"]
    mismatches = []
    for path, expected in metadata["provenance"]["files_sha256"].items():
        original = subprocess.run(["git", "show", f"{source_commit}:{path}"], cwd=root, capture_output=True, check=True).stdout
        if hashlib.sha256(original).hexdigest() != expected:
            mismatches.append(path)
    if mismatches:
        raise ValueError(f"Recorded inputs/source differ from {source_commit}: {mismatches}")
    results = {"schema_version": 1, "source_commit": source_commit, "source_hashes_match": True,
               "layouts": {}, "traces": {}, "historical_completion_counter_differences": 0}
    names = ("initial.json", "witness.json", "training-best.json", "evaluation-median.json", "evaluation-best.json")
    for name in names:
        accepted = check(catalog, json.loads((output/name).read_text()))
        if not accepted["valid"]:
            raise ValueError(f"Invalid retained layout {name}: {accepted}")
        results["layouts"][name] = accepted["score"]
    traces = [output/"witness-trace.json", *sorted(output.glob("evaluation-*-representative-trace.json"))]
    if (output/"training-best-trace.json").exists():
        traces.append(output/"training-best-trace.json")
    for path in traces:
        env = Environment(catalog, config)
        total = 0.0
        rows = json.loads(path.read_text())
        for i, row in enumerate(rows):
            command = row["command"]
            if command not in env.candidates():
                raise ValueError(f"Trace command outside the declared domain: {path}:{i+1}")
            reward, _, outcome = env.step(command)
            if abs(reward-row["reward"]) > 1e-12 or any(outcome[k] != row[k] for k in ("applied", "complete", "improved")):
                raise ValueError(f"Reward/transition replay mismatch: {path}:{i+1}")
            # The initial run counted restoration of a prior valid base as a new closure.
            # Its later diagnostic correction changes no geometry, reward, or policy input.
            results["historical_completion_counter_differences"] += outcome["new_completion"] != row["new_completion"]
            total += reward
        accepted = check(catalog, serialize(env.best))
        if not accepted["valid"] or abs(total-(env.initial_score[0]-env.best_score[0])/env.initial_score[0]) > 1e-12:
            raise ValueError(f"Invalid replay outcome: {path}")
        results["traces"][path.name] = {"commands": len(rows), "return": total, "score": accepted["score"]}
    results["status"] = "passed"
    (output/"audit.json").write_text(json.dumps(results, indent=2)+"\n")
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    print(json.dumps(audit(args.root.resolve(), args.case.resolve(), args.output.resolve(), args.source_commit), indent=2))
