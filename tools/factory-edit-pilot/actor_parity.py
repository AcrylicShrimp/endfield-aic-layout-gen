"""Compare eager/compiled actions with identical weights, observations and sampling RNG."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import time

import torch

from acceptance import Catalog
from compiled_actor import CompiledActor
from environment import Environment, serialize
from policy import Policy
from run import provenance, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--checkpoints", required=True, type=Path, nargs="+")
    parser.add_argument("--cache-directory", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f"Refusing to overwrite {args.output}")
    args.output.mkdir(parents=True)
    config = json.loads(args.config.read_text())
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    env = Environment(catalog, config)
    model = Policy(env.observe(env.candidates()))
    compiled = CompiledActor(model, args.cache_directory)
    rows = []
    try:
        for path in args.checkpoints:
            checkpoint = torch.load(path, map_location="cpu", weights_only=False)
            model.load_state_dict(checkpoint["policy"])
            totals = Counter()
            maxima = dict(logits=0.0, probability=0.0, value=0.0)
            started = time.perf_counter()
            for i in range(config["evaluation_seeds"]):
                seed = config["evaluation_seed_start"]+i
                generator = torch.Generator().manual_seed(seed)
                env.reset()
                for step in range(config["horizon"]):
                    candidates = env.candidates()
                    observation = env.observe(candidates)
                    state = generator.get_state()
                    twin = torch.Generator().set_state(state)
                    captured = {}
                    def eager_forward(data):
                        captured["eager"] = model(data)
                        return captured["eager"]
                    def compiled_forward(data):
                        captured["compiled"] = compiled(data)
                        return captured["compiled"]
                    eager_action = model.act(observation, generator, forward=eager_forward)
                    compiled_action = model.act(observation, twin, forward=compiled_forward)
                    a, b = captured["eager"], captured["compiled"]
                    logits_error = (a[0]-b[0]).abs().max().item()
                    probability_error = (a[0].softmax(-1)-b[0].softmax(-1)).abs().max().item()
                    value_error = (a[1]-b[1]).abs().max().item()
                    for key, error in zip(maxima, (logits_error, probability_error, value_error)):
                        maxima[key] = max(maxima[key], error)
                    totals["observations"] += 1
                    totals["same_action"] += eager_action[0] == compiled_action[0]
                    totals["same_rng_after"] += torch.equal(generator.get_state(), twin.get_state())
                    if eager_action[0] != compiled_action[0] and not (args.output/"first-action-difference.pt").exists():
                        torch.save({"checkpoint": str(path), "seed": seed, "step": step,
                                    "observation": observation, "rng": state, "draft": serialize(env.draft),
                                    "eager": a, "compiled": b, "eager_action": eager_action,
                                    "compiled_action": compiled_action}, args.output/"first-action-difference.pt")
                    for actual, expected in zip(b, a):
                        torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
                    env.step(candidates.commands[eager_action[0]].to_dict())
            row = {"checkpoint": str(path), "checkpoint_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                   "counts": dict(totals), "maximum_absolute_difference": maxima,
                   "seconds": time.perf_counter()-started}
            rows.append(row)
            print(json.dumps(row), flush=True)
    finally:
        compiled.close()
    write(args.output/"result.json", {"schema_version": 1, "provenance": provenance(catalog, args.config),
                                    "contract": "Same parameter tensors, observation arrays and CPU RNG state; eager trajectory",
                                    "rows": rows, "compilation": compiled.report()})


if __name__ == "__main__":
    main()
