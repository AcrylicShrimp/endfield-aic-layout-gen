"""Profile environment-only replay; verify every retained observation outside profiling."""

import argparse
import cProfile
import hashlib
from pathlib import Path
import pstats
import json

import numpy as np
import torch

from acceptance import Catalog
from environment import Environment
from run import provenance, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path, nargs="+")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1:
        parser.error("output must be fresh and repeats positive")
    config = json.loads(args.config.read_text())
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    rows = []
    for path in args.input:
        saved = torch.load(path, map_location="cpu", weights_only=False)["rollout"]
        profile = cProfile.Profile()
        for _ in range(args.repeats):
            env = Environment(catalog, config)
            actual = []
            profile.enable()
            for action in saved["actions"]:
                candidates = env.candidates()
                actual.append(env.observe(candidates))
                _, done, result = env.step(candidates.commands[action].to_dict())
                if not result["applied"]:
                    raise ValueError(f"profile.replay_failed: {path}: action {action}: {result}")
                if done:
                    env.reset()
            profile.disable()
            for observation, expected in zip(actual, saved["observations"], strict=True):
                for key in expected:
                    np.testing.assert_array_equal(observation[key], expected[key])
        stats = pstats.Stats(profile)
        functions = [{"file": filename, "line": line, "function": function,
                      "primitive_calls": primitive, "calls": calls, "self_seconds": own,
                      "cumulative_seconds": cumulative}
                     for (filename, line, function), (primitive, calls, own, cumulative, _) in stats.stats.items()]
        rows.append({"input": str(path), "input_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                     "observations": len(saved["observations"])*args.repeats, "observation_parity": True,
                     "profiled_seconds": stats.total_tt, "functions": sorted(functions, key=lambda r: -r["self_seconds"]),
                     "last_replay_counters": env.counters})
        print(json.dumps({key: value for key, value in rows[-1].items() if key != "functions"}), flush=True)
    args.output.mkdir(parents=True)
    write(args.output/"result.json", {"schema_version": 1, "provenance": provenance(catalog, args.config),
                                    "scope": "Environment-only replay; profiler overhead included, no throughput claim",
                                    "repeats": args.repeats, "rows": rows})


if __name__ == "__main__":
    main()
