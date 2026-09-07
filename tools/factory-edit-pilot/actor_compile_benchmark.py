"""Measure dynamic compiled CPU inference, including startup cost and weight refresh."""

import argparse
import hashlib
import json
from pathlib import Path
import statistics
import time

import torch

from actor_benchmark import measure
from compiled_actor import CompiledActor
from policy import Policy, single
from acceptance import Catalog
from run import provenance, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cache-directory", required=True, type=Path)
    parser.add_argument("--threads", type=int, default=16)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1:
        parser.error("output must be fresh and repeats positive")
    args.output.mkdir(parents=True)
    torch.set_num_threads(args.threads)
    torch.set_num_interop_threads(1)
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    saved = torch.load(args.input, map_location="cpu", weights_only=False)
    observations = saved["rollout"]["observations"]
    eager = Policy(observations[0])
    eager.load_state_dict(saved["policy"])
    started = time.perf_counter()
    compiled = CompiledActor(eager, args.cache_directory)
    startup = measure(compiled, observations, 1, "cpu", False)
    startup_seconds = time.perf_counter()-started
    print(json.dumps({"startup_seconds": startup_seconds}), flush=True)
    compiled.phase = "steady"
    rows = []
    for repeat in range(args.repeats):
        for label, model in [("eager", eager), ("compiled", compiled)][::1 if repeat % 2 == 0 else -1]:
            result = measure(model, observations, 1, "cpu", False)
            rows.append({"implementation": label, "repeat": repeat, **result})
            print(json.dumps(rows[-1]), flush=True)
    maximum_difference = 0.0
    with torch.no_grad():
        # Every retained dynamic shape plus an explicit actor-weight refresh must agree.
        for refresh in (False, True):
            if refresh:
                compiled.phase = "weight_refresh"
                eager.load_state_dict({k: value+0.001 for k, value in saved["policy"].items()})
            for observation in observations:
                data = single(observation)
                for actual, expected in zip(compiled(data), eager(data)):
                    maximum_difference = max(maximum_difference, (actual-expected).abs().max().item())
                    torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
    summary = {label: statistics.median(r["seconds"] for r in rows if r["implementation"] == label)
               for label in ("eager", "compiled")}
    write(args.output/"result.json", {"schema_version": 1, "provenance": provenance(catalog, args.config),
                                    "input_sha256": hashlib.sha256(args.input.read_bytes()).hexdigest(),
                                    "startup_seconds": startup_seconds, "startup": startup, "rows": rows,
                                    "maximum_absolute_difference": maximum_difference,
                                    "weight_refresh": "passed", "summary": summary,
                                    "compilation": compiled.report()})
    compiled.close()
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
