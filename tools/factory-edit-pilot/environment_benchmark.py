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

from reference.acceptance import Catalog
from reference.environment import Environment, serialize
from reference.commands import CandidateSet
from evidence import provenance, write


def replay(environment, catalog, config, rollout, reset_caches=False):
    env = environment(catalog, config)
    rows = []
    cache_hits, cache_misses = 0, 0
    sort_hits, sort_misses = 0, 0
    started = time.perf_counter()
    for selected in rollout["actions"]:
        candidates = env.candidates()
        observation = env.observe(candidates)
        typed = isinstance(candidates, CandidateSet)
        result = env.step(candidates.commands[selected].to_dict() if typed else candidates[selected])
        rows.append((candidates.commands if typed else candidates, observation, result))
        if result[1]:
            env.reset()
            if reset_caches:
                if hasattr(env, "encode_candidate"):
                    info = env.encode_candidate.cache_info()
                    cache_hits += info.hits
                    cache_misses += info.misses
                    env.encode_candidate.cache_clear()
                    if hasattr(env, "sort_keys"):
                        env.sort_keys.clear()
                    else:
                        info = env.canonical_command_key.cache_info()
                        sort_hits += info.hits
                        sort_misses += info.misses
                        env.canonical_command_key.cache_clear()
                if hasattr(env, "feature_templates"):
                    env.feature_templates.clear()
    elapsed = time.perf_counter()-started
    states = [serialize(s) for s in (env.draft, env.base, env.best)]
    counts = dict(env.counters)
    if hasattr(env, "encode_candidate"):
        info = env.encode_candidate.cache_info()
        counts.update(encoding_cache_hits=cache_hits+info.hits, encoding_cache_misses=cache_misses+info.misses,
                      encoding_cache_entries=info.currsize,
                      sort_cache_entries=len(env.sort_keys) if hasattr(env, "sort_keys") else env.canonical_command_key.cache_info().currsize)
    if hasattr(env, "canonical_command_key") and hasattr(env.canonical_command_key, "cache_info"):
        info = env.canonical_command_key.cache_info()
        counts.update(sort_cache_hits=sort_hits+info.hits, sort_cache_misses=sort_misses+info.misses)
    # Full external serialization is diagnostic work, outside the production timing.
    rows = [([command.to_dict() for command in candidates] if isinstance(candidates, tuple) else candidates, observation, result)
            for candidates, observation, result in rows]
    return rows, states, elapsed, dict(env.seconds), counts


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
    parser.add_argument("--reset-caches-per-episode", action="store_true", help="Measure reuse within an episode only")
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
    # Historical source imports are explicitly scoped to the isolated oracle namespace.
    scoped = source.replace("from acceptance import", "from reference.acceptance import").replace(
        "from commands import", "from reference.commands import")
    exec(compile(scoped, "reference_environment.py", "exec"), reference.__dict__)
    rows = []
    for path in args.input:
        rollout = torch.load(path, weights_only=False, map_location="cpu")["rollout"]
        for repeat in range(args.repeats+1):
            results = {}
            implementations = [("reference", reference.Environment), ("current", Environment)]
            for label, implementation in implementations[::1 if repeat % 2 == 0 else -1]:
                result = replay(implementation, catalog, config, rollout, args.reset_caches_per_episode)
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
                                     "cache_scope": "episode" if args.reset_caches_per_episode else "environment_lifetime",
                                     "reference_sha256": hashlib.sha256(source.encode()).hexdigest(),
                                     "input_sha256": {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in args.input},
                                     "provenance": provenance(catalog, args.config), "parity": "bit_exact",
                                     "rows": rows, "summary": summary})
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
