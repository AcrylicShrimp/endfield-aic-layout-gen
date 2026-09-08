"""Run the frozen curriculum/concurrent-PPO factorial study with original-root evaluations."""

import argparse
from collections import Counter
from copy import deepcopy
import json
from pathlib import Path
import random
import time

import numpy as np
import torch

from curriculum import Collector, remaining_at
from evidence import append, write
from native import Runtime, layout, snapshot, trace
from native_training_comparison import policy_hash
from policy import Policy, update
from run import refresh_actor, verify_environment
from two_case import evaluate_cases, study_provenance, validate_compatibility


def load(root, protocol_path, condition, seed, library):
    protocol = json.loads(protocol_path.read_text())
    if protocol.get("schema_version") != 1 or condition not in protocol["conditions"] or seed not in protocol["training_seeds"]:
        raise ValueError(f"curriculum.protocol: {protocol_path}: incompatible schema, condition or seed")
    base_path = (root / protocol["base_config"]).resolve()
    config = json.loads(base_path.read_text())
    config.update(seed=seed, commands=protocol["commands"], checkpoints=protocol["checkpoints"],
                  rollout_commands=protocol["streams"] * config["horizon"])
    boundaries = [stage["until"] for stage in protocol["curriculum"]]
    if (protocol["streams"] < 2 or protocol["streams"] % len(protocol["cases"])
            or boundaries != sorted(set(boundaries)) or boundaries[-1] >= config["commands"]
            or any(stage["remaining"] not in (1, 2, 3) for stage in protocol["curriculum"])
            or any(n % config["rollout_commands"] for n in [config["commands"], *config["checkpoints"], *boundaries])
            or config["checkpoints"][0] != 0 or config["checkpoints"][-1] != config["commands"]
            or config["evaluation_seeds"] < 2):
        raise ValueError(f"curriculum.budget: {protocol_path}: invalid streams, stage or checkpoint boundaries")
    if config["device"] == "cuda" and not torch.cuda.is_available():
        raise ValueError(f"curriculum.cuda: {base_path}: CUDA is unavailable")
    runtimes = {key: Runtime(root, root / path, config, library) for key, path in protocol["cases"].items()}
    validate_compatibility(runtimes)
    return protocol, base_path, config, runtimes


def train(root, protocol_path, condition, seed, library, output):
    if output.exists():
        raise ValueError(f"curriculum.output_exists: {output}: choose a fresh directory")
    protocol, base_path, config, runtimes = load(root, protocol_path, condition, seed, library)
    parallel = protocol["conditions"][condition]["parallel"]
    torch.set_num_threads(config["threads"])
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    torch.manual_seed(seed)
    np.random.seed(seed)
    random.seed(seed)
    rng = np.random.default_rng(seed)
    output.mkdir(parents=True)
    sources, verification = {}, {}
    for key, runtime in runtimes.items():
        (output / key).mkdir()
        sources[key] = study_provenance(runtime, protocol_path, base_path)
        verification[key] = verify_environment(runtime, config, output / key)
        write(output / key / "training-best.json", layout(runtime.new_engine(), "root"))
        write(output / key / "training-best-trace.json", [])
    example, _ = next(iter(runtimes.values())).new_engine().observe()
    model = Policy(example, normalization=config.get("normalization", "none"))
    initial_hash = policy_hash(model.state_dict())
    evaluation_actor = deepcopy(model)
    model.to(config["device"])
    actor = model if parallel else evaluation_actor
    optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
    collector = Collector(runtimes, config, protocol["streams"], seed, parallel)
    write(output / "starts.json", collector.starts)
    commands, episodes, optimizer_steps, setup_commands = 0, 0, 0, 0
    exposure, stage_counts = Counter(), Counter()
    counts = {key: Counter() for key in runtimes}
    champions = {key: list(runtime.metadata["initial_score"]) for key, runtime in runtimes.items()}
    seconds = {key: 0.0 for key in ("evaluation", "collection", "update", "actor_snapshot")}
    phase, rollout = "setup", None
    started = time.perf_counter()

    def metadata(status, error=None):
        value = {"schema_version": 1, "experiment_family": "curriculum_parallel", "status": status,
                 "condition": condition, "config": config, "protocol": protocol, "seed": seed,
                 "initial_policy_sha256": initial_hash, "provenance": sources, "verification": verification,
                 "completed_commands": commands, "episodes": episodes, "optimizer_steps": optimizer_steps,
                 "setup_commands": setup_commands, "case_commands": dict(exposure), "stage_commands": dict(stage_counts),
                 "counts": {key: dict(value) for key, value in counts.items()}, "champions": champions,
                 "seconds": dict(seconds), "collector_seconds": dict(collector.seconds),
                 "training_seconds": time.perf_counter() - started - seconds["evaluation"],
                 "actor_device": str(next(actor.parameters()).device), "evaluation_device": "cpu",
                 "native_workers": protocol["streams"] if parallel else 1,
                 "parameters": model.parameter_count, "dimensions": model.dimensions}
        if error:
            value["error"] = error
        return value

    def save(kind):
        path = output / (f"checkpoint-{commands}.pt" if kind == "complete_rollout" else "interrupted-state.pt")
        value = {"schema_version": 1, "kind": kind, "phase": phase, "metadata": metadata("running"),
                 "policy": model.state_dict(), "evaluation_policy": evaluation_actor.state_dict(),
                 "optimizer": optimizer.state_dict(), "torch_rng": torch.get_rng_state(),
                 "stream_rngs": [g.get_state() for g in collector.generators], "numpy_rng": rng.bit_generator.state,
                 "numpy_legacy_rng": np.random.get_state(), "python_rng": random.getstate(),
                 "cuda_rng": torch.cuda.get_rng_state_all() if config["device"] == "cuda" else None,
                 "environments": [snapshot(env) for env in collector.envs],
                 "partial_collection": collector.partial if kind != "complete_rollout" else None,
                 "partial_rollout": rollout if kind != "complete_rollout" else None,
                 "partial_completed_steps": collector.completed_steps}
        value["pending_transition"] = collector.pending if kind != "complete_rollout" else None
        temporary = path.with_suffix(".tmp")
        torch.save(value, temporary)
        temporary.replace(path)

    def checkpoint():
        nonlocal phase
        phase = "checkpoint"
        save("complete_rollout")
        phase = "evaluation"
        clock = time.perf_counter()
        try:
            evaluate_cases(evaluation_actor, runtimes, config, output, commands,
                           {key: exposure[key] for key in runtimes}, clock - started - seconds["evaluation"])
        finally:
            seconds["evaluation"] += time.perf_counter() - clock
        record("running")

    def record(status, error=None):
        value = metadata(status, error)
        write(output / "metadata.json", value)
        for key in runtimes:
            write(output / key / "metadata.json", {**value, "case": key,
                  "provenance": sources[key], "verification": verification[key]})
        return value

    try:
        checkpoint()
        while commands < config["commands"]:
            remaining = remaining_at(protocol, condition, commands)
            phase = "collection"
            clock = time.perf_counter()
            rollout, rows = collector.collect(actor, [remaining] * len(collector.envs))
            seconds["collection"] += time.perf_counter() - clock
            for key, env, row in zip(collector.keys, collector.envs, rows):
                commands += config["horizon"]
                episodes += 1
                exposure[key] += config["horizon"]
                stage_counts[f"{key}:{remaining}"] += config["horizon"]
                prefix = collector.starts[key][remaining]
                setup_commands += len(prefix)
                stats = json.loads(env.episode_json())
                total = sum(row["rewards"])
                if abs(total - (env.initial_score[0] - env.best_score[0]) / env.initial_score[0]) > 1e-10:
                    raise RuntimeError(f"curriculum.reward: {key}: sampled reward differs from retained best")
                counts[key].update(stats["counts"])
                evidence = {"case": key, "episode": episodes, "commands": commands, "remaining": remaining,
                            "setup_commands": len(prefix), "return": total, "score": list(env.best_score),
                            "counts": stats["counts"], "unfinished": not stats["draft_acceptance"]["valid"]}
                append(output / "episodes.jsonl", evidence)
                if list(env.best_score) < champions[key]:
                    champions[key] = list(env.best_score)
                    write(output / key / "training-best.json", layout(env, "best"))
                    write(output / key / "training-best-trace.json", trace(env))
                    write(output / key / "training-best-start.json", {"prefix": prefix, **evidence})
            phase = "update"
            clock = time.perf_counter()
            metrics = update(model, optimizer, rollout, config, rng)
            seconds["update"] += time.perf_counter() - clock
            optimizer_steps += metrics["optimizer_steps"]
            append(output / "updates.jsonl", {"commands": commands, "remaining": remaining, **metrics})
            phase = "actor_snapshot"
            clock = time.perf_counter()
            refresh_actor(model, evaluation_actor)
            seconds["actor_snapshot"] += time.perf_counter() - clock
            if commands in config["checkpoints"]:
                checkpoint()
                print(json.dumps({"event": "curriculum_progress", "condition": condition, "seed": seed,
                                  "commands": commands, "remaining": remaining, "champions": champions}), flush=True)
        result = record("complete")
        return result
    except BaseException as error:
        collector.close()
        diagnostic = {"code": "curriculum.interrupted", "severity": "error", "stage": phase,
                      "entity": str(output), "message": str(error) or type(error).__name__}
        record("interrupted", diagnostic)
        save("interrupted")
        raise
    finally:
        collector.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--condition", required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    train(args.root.resolve(), args.protocol.resolve(), args.condition, args.seed,
          args.native_library.resolve(), args.output.resolve())


if __name__ == "__main__":
    main()
