"""Fresh uniformly sampled nonmonotone trajectories on full generated factories."""

import argparse
import json
from pathlib import Path
import random
import time

from client import Pool


def run(options):
    output = Path(options.output_directory).resolve()
    output.mkdir()
    began = time.perf_counter()
    rows = []
    with Pool(Path(options.roots).resolve(), output, options.workers) as pool:
        roots = [r for r in pool.metadata["roots"] if options.split == "all" or r["split"] == options.split]
        if not roots:
            raise ValueError(f"No roots in split {options.split}")
        jobs = [(r, repeat) for repeat in range(options.repeats) for r in roots]
        for start in range(0, len(jobs), options.workers):
            batch = jobs[start:start + options.workers]
            # The final partial batch runs only its actual workers; padding is not fake evidence.
            workers = pool.workers[:len(batch)]
            generators = [random.Random(options.seed + i + start) for i in range(len(batch))]
            futures = [pool.executor.submit(w.call, "reset", root=r["id"], budget=dict(steps=options.steps, elapsed_ms=options.episode_ms))
                       for w, (r, _) in zip(workers, batch)]
            obs = [f.result() for f in futures]
            while any(not o["done"] for o in obs):
                futures = []
                for i, (w, o, generator) in enumerate(zip(workers, obs, generators)):
                    if o["done"]:
                        continue
                    valid = [a for a, yes in enumerate(o["features"]["action_mask"]) if yes]
                    futures.append((i, pool.executor.submit(w.call, "step", action=generator.choice(valid))))
                for i, future in futures:
                    obs[i] = future.result()
            for w, (root, repeat) in zip(workers, batch):
                report = w.call("summary")
                report.update(split=root["split"], generation=root["generation"], repeat=repeat,
                              artifact=w.call("save"), audit=w.call("audit"))
                rows.append(report)
            print(json.dumps(dict(completed=len(rows), total=len(jobs), elapsed_seconds=time.perf_counter() - began)), flush=True)
    result = dict(schema_version=1, policy="uniform-valid-mask-nonmonotone", options=vars(options),
                  elapsed_seconds=time.perf_counter() - began, rows=rows)
    with output.joinpath("report.json").open("x") as stream:
        json.dump(result, stream)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--roots", required=True)
    parser.add_argument("--output-directory", required=True)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--steps", type=int, default=32)
    parser.add_argument("--episode-ms", type=int, default=10000)
    parser.add_argument("--repeats", type=int, default=1)
    parser.add_argument("--seed", type=int, default=7)
    parser.add_argument("--split", choices=["all", "train", "validation", "test", "out-of-family"], default="train")
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="rl-rollout-failed", severity="error", stage="rollout", message=str(error), entity=None)))
        raise SystemExit(1)
