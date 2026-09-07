"""Measure complete serial/batched collection before changing the training runner."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import statistics
import time

import torch

from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize
from policy import Policy, batch, single
from evidence import provenance, write


def collect(model, catalog, config, size, device, inference):
    if config["rollout_commands"] % (size*config["horizon"]):
        raise ValueError("collection.batch_boundary: rollout must contain complete episode groups")
    envs = [Environment(catalog, config) for _ in range(size)]
    generator = torch.Generator().manual_seed(700017)
    streams = []
    inference_seconds = 0.0
    context = torch.inference_mode() if inference else torch.no_grad()
    started = time.perf_counter()
    with context:
        for _ in range(config["rollout_commands"]//(size*config["horizon"])):
            group = [[] for _ in envs]
            for _ in range(config["horizon"]):
                candidates = [env.candidates() for env in envs]
                observations = [env.observe(c) for env, c in zip(envs, candidates)]
                begin = time.perf_counter()
                data = single(observations[0]) if size == 1 else batch(observations)
                data = {key: value.to(device) for key, value in data.items()}
                logits, values = model(data)
                log_probs, values = logits.log_softmax(-1).cpu(), values.cpu()
                selected = [torch.multinomial(row.exp(), 1, generator=generator).item() for row in log_probs]
                rows = [(action, log_probs[i, action].item(), values[i].item()) for i, action in enumerate(selected)]
                inference_seconds += time.perf_counter()-begin
                for i, (env, candidate, (action, log_prob, value)) in enumerate(zip(envs, candidates, rows)):
                    command = candidate.commands[action].to_dict()
                    reward, done, outcome = env.step(command)
                    group[i].append({"command": command, "action": action, "candidate_count": len(candidate.commands),
                                     "log_probability": log_prob, "value": value, "reward": reward, "done": done})
            for env, rows in zip(envs, group):
                streams.append({"rows": rows, "best": serialize(env.best), "draft": serialize(env.draft),
                                "counts": dict(env.episode_counts), "score": env.best_score})
                env.reset()
    elapsed = time.perf_counter()-started
    # Independent acceptance and command replay are outside the collection timing.
    for stream in streams:
        if not check(catalog, stream["best"])["valid"]:
            raise ValueError("collection.acceptance: best layout invalid")
        replay = Environment(catalog, config)
        for row in stream["rows"]:
            reward, done, outcome = replay.step(row["command"])
            if not outcome["applied"] or (reward, done) != (row["reward"], row["done"]):
                raise ValueError("collection.replay: transition differs")
        if serialize(replay.draft) != stream["draft"] or serialize(replay.best) != stream["best"]:
            raise ValueError("collection.replay: final layout differs")
    return {"seconds": elapsed, "inference_seconds": inference_seconds,
            "environment_seconds": dict(sum((env.seconds for env in envs), Counter())),
            "environment_counters": dict(sum((env.counters for env in envs), Counter())),
            "streams": streams, "audit": "passed"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1:
        parser.error("output must be fresh and repeats positive")
    args.output.mkdir(parents=True)
    config = json.loads(args.config.read_text())
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    saved = torch.load(args.input, map_location="cpu", weights_only=False)
    models = {}
    for device in ("cpu", "cuda"):
        model = Policy(saved["rollout"]["observations"][0]).to(device)
        model.load_state_dict(saved["policy"])
        models[device] = model
    variants = [("cpu", 1, False), ("cpu", 2, False), ("cuda", 2, False), ("cuda", 2, True)]
    rows = []
    for repeat in range(args.repeats+1):
        for device, size, inference in variants[::1 if repeat % 2 == 0 else -1]:
            result = collect(models[device], catalog, config, size, device, inference)
            row = {"device": device, "batch_size": size, "inference_mode": inference,
                   "repeat": repeat, "warmup": repeat == 0, **result}
            rows.append(row)
            print(json.dumps({k: v for k, v in row.items() if k != "streams"}), flush=True)
    summary = [{"device": device, "batch_size": size, "inference_mode": inference,
                "seconds": statistics.median(r["seconds"] for r in rows if not r["warmup"]
                                             and (r["device"], r["batch_size"], r["inference_mode"]) == (device, size, inference))}
               for device, size, inference in variants]
    write(args.output/"result.json", {"schema_version": 1, "provenance": provenance(catalog, args.config),
                                    "input_sha256": hashlib.sha256(args.input.read_bytes()).hexdigest(),
                                    "rows": rows, "summary": summary,
                                    "peak_cuda_allocated_bytes": torch.cuda.max_memory_allocated()})
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
