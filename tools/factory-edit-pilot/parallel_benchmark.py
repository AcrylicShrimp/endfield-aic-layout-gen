"""Fixed-action replay separates GPU batching from concurrent native environment work."""

import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import statistics
import time

import numpy as np
import torch

from curriculum import Collector, act_batch
from curriculum_study import load
from evidence import write
from native import snapshot
from policy import Policy
from two_case import study_provenance
from native_training_comparison import policy_hash


def run(root, protocol_path, library, output, repeats):
    if output.exists():
        raise ValueError(f"parallel_benchmark.output_exists: {output}")
    protocol = json.loads(protocol_path.read_text())
    seed = protocol["training_seeds"][0]
    _, base_path, config, runtimes = load(root, protocol_path, "root-parallel", seed, library)
    torch.set_num_threads(config["threads"])
    torch.manual_seed(seed)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    example = next(iter(runtimes.values())).new_engine().observe()[0]
    cpu = Policy(example)
    device_model = deepcopy(cpu).to(config["device"])
    output.mkdir(parents=True)
    trials, replay = [], {}
    for remaining in (0, 1):
        recorder = Collector(runtimes, config, protocol["streams"], seed, False)
        rng = np.random.default_rng(seed)
        actions = [[] for _ in recorder.envs]
        for i, (env, key) in enumerate(zip(recorder.envs, recorder.keys)):
            env.reset_with_prefix_json(json.dumps(recorder.starts[key][remaining]))
            for _ in range(config["horizon"]):
                handle, count = env.candidates()
                index = int(rng.integers(count))
                env.step(handle, index)
                actions[i].append(index)
        expected = [{k: snapshot(env)[k] for k in ("draft", "best", "best_score", "steps", "episode_counts")}
                    for env in recorder.envs]
        recorder.close()
        replay[str(remaining)] = {"actions": actions, "expected_states": expected}
        # Round-robin order reduces fixed thermal/cache ordering bias; first repeat is warm-up.
        modes = [("cpu-serial", False, False), ("gpu-batch-serial-native", True, False), ("gpu-batch-threaded-native", True, True)]
        for repeat in range(repeats + 1):
            order = modes[repeat % len(modes):] + modes[:repeat % len(modes)]
            for name, batched, threaded in order:
                collector = Collector(runtimes, config, protocol["streams"], seed, batched, threaded)
                try:
                    for env, key in zip(collector.envs, collector.keys):
                        env.reset_with_prefix_json(json.dumps(collector.starts[key][remaining]))
                    model = device_model if batched else cpu
                    if config["device"] == "cuda":
                        torch.cuda.synchronize()
                    started = time.perf_counter()
                    groups = [list(range(protocol["streams"]))] if batched else [[i] for i in range(protocol["streams"])]
                    for indices in groups:
                        for t in range(config["horizon"]):
                            observed = collector.mapped(lambda i: collector.envs[i].observe(), indices)
                            if batched:
                                act_batch(model, [p[0] for p in observed], [collector.generators[i] for i in indices])
                            else:
                                model.act(observed[0][0], collector.generators[indices[0]])
                            collector.mapped(lambda item: collector.envs[item[0]].step(item[1], actions[item[0]][t]),
                                             [(i, p[1]) for i, p in zip(indices, observed)])
                    if config["device"] == "cuda":
                        torch.cuda.synchronize()
                    elapsed = time.perf_counter() - started
                    actual = [{k: snapshot(env)[k] for k in expected[0]} for env in collector.envs]
                    if actual != expected:
                        raise RuntimeError(f"parallel_benchmark.parity: {name}: fixed-action final states differ")
                    if repeat:
                        trials.append({"remaining": remaining, "mode": name, "repeat": repeat, "seconds": elapsed})
                finally:
                    collector.close()
    summaries = [{"remaining": remaining, "mode": name,
                  "median_seconds": statistics.median(row["seconds"] for row in trials if row["remaining"] == remaining and row["mode"] == name)}
                 for remaining in (0, 1) for name, _, _ in modes]
    result = {"schema_version": 1, "status": "passed", "trials": trials, "summaries": summaries,
              "edits_per_trial": protocol["streams"] * config["horizon"], "repeats": repeats,
              "parity": "Identical recorded candidate indices and final geometry, scores, steps and episode counters",
              "timing_scope": "Observation, actual inference/sampling (discarded), and fixed native transitions; excludes setup and verification",
              "native": {key: runtime.provenance for key, runtime in runtimes.items()},
              "provenance": {key: study_provenance(runtime, protocol_path, base_path) for key, runtime in runtimes.items()},
              "initial_policy_sha256": policy_hash(cpu.state_dict()), "config": config,
              "protocol_sha256": hashlib.sha256(protocol_path.read_bytes()).hexdigest()}
    write(output / "fixed-replay.json", replay)
    result["fixed_replay_sha256"] = hashlib.sha256((output / "fixed-replay.json").read_bytes()).hexdigest()
    write(output / "benchmark.json", result)
    print(json.dumps(summaries, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=5)
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    torch.set_num_interop_threads(1)
    run(args.root.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve(), args.repeats)


if __name__ == "__main__":
    main()
