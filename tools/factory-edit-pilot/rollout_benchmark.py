"""Time interleaved CPU inference/environment work on an identical recorded state sequence."""

import argparse
import json
from pathlib import Path
import statistics
import subprocess
import types
import hashlib
import time

import numpy as np
import torch

from acceptance import Catalog
from environment import Environment
from policy import Policy
from run import provenance, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--threads", type=int, nargs="+", default=[1, 2, 4, 8, 16, 24])
    parser.add_argument("--reference-policy", help="Trusted historical policy source for an inference comparison")
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    if args.output.exists():
        raise ValueError(f"Refusing to overwrite {args.output}")
    if args.repeats < 1 or any(t < 1 for t in args.threads):
        parser.error("repeats and threads must be positive")
    args.output.mkdir(parents=True)
    config = json.loads(args.config.read_text())
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    saved = torch.load(args.input, map_location="cpu", weights_only=False)
    rollout = saved["rollout"]
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    model = Policy(rollout["observations"][0])
    model.load_state_dict(saved["policy"])
    variants = [("current", model)]
    reference_hash = None
    if args.reference_policy:
        source = subprocess.run(["git", "show", f"{args.reference_policy}:tools/factory-edit-pilot/policy.py"],
                                cwd=catalog.root, capture_output=True, text=True, check=True).stdout
        reference = types.ModuleType("reference_policy")
        exec(compile(source, "reference_policy.py", "exec"), reference.__dict__)
        reference_model = reference.Policy(rollout["observations"][0])
        reference_model.load_state_dict(saved["policy"])
        variants.insert(0, ("reference", reference_model))
        reference_hash = hashlib.sha256(source.encode()).hexdigest()
    rows = []
    for repeat in range(args.repeats+1):
        for threads in args.threads[::1 if repeat % 2 == 0 else -1]:
            for label, model in variants[::1 if repeat % 2 == 0 else -1]:
                torch.set_num_threads(threads)
                env = Environment(catalog, config)
                observations = []
                generator = torch.Generator().manual_seed(700017)
                inference_seconds = 0.0
                started = time.perf_counter()
                for recorded in rollout["actions"]:
                    candidates = env.candidates()
                    observation = env.observe(candidates)
                    inference_started = time.perf_counter()
                    # Execute recorded actions to fix geometry independently of numeric sampling differences.
                    model.act(observation, generator)
                    inference_seconds += time.perf_counter()-inference_started
                    _, done, result = env.step(candidates.commands[recorded].to_dict())
                    if not result["applied"]:
                        raise ValueError(f"Recorded replay action failed: {candidates.commands[recorded].to_dict()}")
                    observations.append(observation)
                    if done:
                        env.reset()
                elapsed = time.perf_counter()-started
                # Verify every state/feature after the timed loop, not merely the final layout.
                for actual, expected in zip(observations, rollout["observations"]):
                    for key in expected:
                        np.testing.assert_array_equal(actual[key], expected[key])
                row = {"threads": threads, "implementation": label, "repeat": repeat, "warmup": repeat == 0,
                       "rollout_seconds": elapsed, "inference_seconds": inference_seconds,
                       "environment_seconds": dict(env.seconds), "states_equal": True}
                rows.append(row)
                print(json.dumps(row), flush=True)
    summary = [{"threads": t, "implementation": label,
                **{key: statistics.median(r[key] for r in rows if r["threads"] == t
                                         and r["implementation"] == label and not r["warmup"])
                   for key in ("rollout_seconds", "inference_seconds")}}
               for t in args.threads for label, _ in variants]
    write(args.output/"result.json", {"schema_version": 1, "config": config, "input": str(args.input),
                                     "reference_policy": args.reference_policy, "reference_policy_sha256": reference_hash,
                                     "provenance": provenance(catalog, args.config), "rows": rows, "summary": summary})
    print(json.dumps({"summary": summary}), flush=True)


if __name__ == "__main__":
    main()
