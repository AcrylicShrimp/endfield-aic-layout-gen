"""Compare exact environment outputs and cost against a trusted historical source."""

import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time
import types

import numpy as np
import torch

from acceptance import Catalog
from environment import Environment, serialize
from run import provenance, write


def replay(environment, catalog, config, rollout):
    env = environment(catalog, config)
    rows = []
    started = time.perf_counter()
    for selected in rollout["actions"]:
        candidates = env.candidates()
        observation = env.observe(candidates)
        result = env.step(candidates[selected])
        rows.append((candidates, observation, result))
        if result[1]:
            env.reset()
    elapsed = time.perf_counter()-started
    states = [serialize(s) for s in (env.draft, env.base, env.best)]
    return rows, states, elapsed, dict(env.seconds), dict(env.counters)


def compare_rows(actual, expected, recorded):
    if actual[1] != expected[1] or len(actual[0]) != len(expected[0]):
        raise ValueError("environment.parity: final states or row counts differ")
    for i, (a, b) in enumerate(zip(actual[0], expected[0])):
        if a[0] != b[0] or a[2] != b[2]:
            raise ValueError(f"environment.parity: candidate order or transition differs at {i}")
        for key, value in a[1].items():
            for wanted in (b[1][key], recorded["observations"][i][key]):
                if value.dtype != wanted.dtype or value.shape != wanted.shape or value.tobytes() != wanted.tobytes():
                    raise ValueError(f"environment.parity: observation differs at {i}: {key}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--reference-source", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1:
        parser.error("output must be fresh and repeats positive")
    args.output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[2]
    config = json.loads(args.config.read_text())
    catalog = Catalog(root, args.case.resolve())
    source = subprocess.run(["git", "show", f"{args.reference_source}:tools/factory-edit-pilot/environment.py"],
                            cwd=root, capture_output=True, check=True, text=True).stdout
    reference = types.ModuleType("reference_environment")
    exec(compile(source, "reference_environment.py", "exec"), reference.__dict__)
    rows = []
    for path in args.input:
        rollout = torch.load(path, weights_only=False, map_location="cpu")["rollout"]
        for repeat in range(args.repeats+1):
            results = {}
            implementations = [("reference", reference.Environment), ("current", Environment)]
            for label, implementation in implementations[::1 if repeat % 2 == 0 else -1]:
                result = replay(implementation, catalog, config, rollout)
                results[label] = result
                rows.append({"input": str(path), "implementation": label, "repeat": repeat,
                             "warmup": repeat == 0, "seconds": result[2], "environment_seconds": result[3],
                             "environment_counters": result[4]})
            compare_rows(results["current"], results["reference"], rollout)
        print(json.dumps({"input": str(path), "commands": len(rollout["actions"]), "parity": "bit_exact"}), flush=True)
    summary = [{"input": str(path), **{label: statistics.median(row["seconds"] for row in rows
                if row["input"] == str(path) and row["implementation"] == label and not row["warmup"])
                for label in ("reference", "current")}} for path in args.input]
    write(args.output/"result.json", {"schema_version": 1, "reference_source": args.reference_source,
                                     "reference_sha256": hashlib.sha256(source.encode()).hexdigest(),
                                     "input_sha256": {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in args.input},
                                     "provenance": provenance(catalog, args.config), "parity": "bit_exact",
                                     "rows": rows, "summary": summary})
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
