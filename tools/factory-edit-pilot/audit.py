"""Audit retained layouts/traces against original source hashes and exact acceptance."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize


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
    for path, expected in metadata["provenance"].get("external_files_sha256", {}).items():
        if hashlib.sha256(Path(path).read_bytes()).hexdigest() != expected:
            raise ValueError(f"Recorded external input changed: {path}")
    for path in [catalog.path, *catalog.paths.values()]:
        key = str(path.relative_to(root)) if path.is_relative_to(root) else str(path)
        field = "files_sha256" if path.is_relative_to(root) else "external_files_sha256"
        if hashlib.sha256(path.read_bytes()).hexdigest() != metadata["provenance"].get(field, {}).get(key):
            raise ValueError(f"Audit runtime input differs from recorded data: {path}")
    native = metadata["provenance"].get("native")
    if native:
        for path, expected in native["build"]["files_sha256"].items():
            if metadata["provenance"]["files_sha256"].get(path) != expected:
                raise ValueError(f"Embedded native build source differs from recorded source: {path}")
        library = Path(native["library"])
        if not library.is_file() or hashlib.sha256(library.read_bytes()).hexdigest() != native["library_sha256"]:
            raise ValueError(f"Native build artifact differs from recorded library: {library}")
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
            if command not in env.candidates().to_dicts():
                raise ValueError(f"Trace command outside the declared domain: {path}:{i+1}")
            reward, _, outcome = env.step(command)
            if abs(reward-row["reward"]) > 1e-12 or any(outcome[k] != row[k] for k in ("applied", "complete", "improved")):
                raise ValueError(f"Reward/transition replay mismatch: {path}:{i+1}")
            # The initial run counted restoration of a prior valid base as a new closure.
            # Its later diagnostic correction changes no geometry, reward, or policy input.
            if native and (reward != row["reward"] or outcome["new_completion"] != row["new_completion"]
                           or outcome["diagnostics"] != row["diagnostics"]):
                raise ValueError(f"Native outcome/diagnostic parity mismatch: {path}:{i+1}")
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
