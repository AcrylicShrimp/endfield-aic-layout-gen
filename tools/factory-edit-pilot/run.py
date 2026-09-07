"""Run the bounded factory edit pilot and retain raw, replayable CPU/CUDA evidence."""

import argparse
from collections import Counter
from copy import deepcopy
import hashlib
import json
import platform
from pathlib import Path
import random
import resource
import subprocess
import time

import numpy as np
import torch

from acceptance import Catalog, check
from environment import Environment, serialize
from policy import Policy, update


def write(path, value):
    path = Path(path)
    temp = path.with_suffix(path.suffix+".tmp")
    temp.write_text(json.dumps(value, indent=2)+"\n")
    temp.replace(path)


def append(path, value):
    with Path(path).open("a") as stream:
        stream.write(json.dumps(value)+"\n")


def verify_environment(catalog, config, output):
    env = Environment(catalog, config)
    write(output/"initial.json", serialize(env.root))
    reward = 0.0
    for command in catalog.case["witness"]:
        if command not in env.candidates():
            raise RuntimeError(f"Witness action outside the model's actual domain: {command}")
        r, done, result = env.step(command)
        if not result["applied"] or done:
            raise RuntimeError(f"Witness action failed: {command}: {result}")
        reward += r
    result = check(catalog, serialize(env.best))
    if not result["valid"] or result["score"][0] >= env.initial_score[0]:
        raise RuntimeError(f"No independently validated improving witness: {result}")
    if abs(reward-(env.initial_score[0]-result["score"][0])/env.initial_score[0]) > 1e-12:
        raise RuntimeError("Witness reward does not telescope to final outcome")
    write(output/"witness.json", serialize(env.best))
    write(output/"witness-trace.json", env.trace)
    return {"initial": check(catalog, serialize(env.root)), "witness": result,
            "witness_return": reward, "witness_commands": len(catalog.case["witness"])}


def evaluate(model, catalog, config, output, commands, training_seconds):
    started = time.perf_counter()
    env = Environment(catalog, config)
    rows, layouts, traces = [], [], []
    model.eval()
    with torch.no_grad():
        for i in range(config["evaluation_seeds"]):
            seed = config["evaluation_seed_start"]+i
            generator = torch.Generator(device="cpu").manual_seed(seed)
            env.reset()
            total = 0.0
            for _ in range(config["horizon"]):
                candidates = env.candidates()
                observation = env.observe(candidates)
                selected, _, _ = model.act(observation, generator)
                reward, _, _ = env.step(candidates[selected])
                total += reward
            layout = serialize(env.best)
            accepted = check(catalog, layout)
            if not accepted["valid"]:
                raise RuntimeError(f"Evaluation export failed independent acceptance: {accepted}")
            row = {"commands": commands, "rollout_seed": seed, "return": total,
                   "score": accepted["score"], "counts": dict(env.episode_counts)}
            append(output/"evaluations.jsonl", row)
            rows.append(row)
            layouts.append(layout)
            traces.append(deepcopy(env.trace))
    returns = np.array([r["return"] for r in rows])
    summary = {"commands": commands, "training_seconds": training_seconds, "mean_return": float(returns.mean()),
               "median_return": float(np.median(returns)), "std_return": float(returns.std(ddof=1)),
               "max_return": float(returns.max()), "improvement_frequency": float((returns > 0).mean()),
               "evaluation_episodes": len(rows), "evaluation_seconds": time.perf_counter()-started,
               "best_score": min(r["score"] for r in rows), "counts": dict(sum((Counter(r["counts"]) for r in rows), Counter()))}
    append(output/"evaluation-summary.jsonl", summary)
    ordered = sorted(range(len(rows)), key=lambda i: (rows[i]["score"], i))
    median_index = ordered[len(ordered)//2]
    write(output/"evaluation-median.json", layouts[median_index])
    write(output/"evaluation-best.json", layouts[ordered[0]])
    write(output/f"evaluation-{commands}-representative-trace.json", traces[median_index])
    model.train()
    print(json.dumps({"event": "evaluation", **summary}), flush=True)
    return summary


def provenance(catalog, config_path):
    directory = Path(__file__).resolve().parent
    files = sorted(set([*directory.glob("*.py"), *directory.glob("*.toml"), *directory.glob("*.lock"),
                        catalog.path.resolve(), config_path.resolve(), *catalog.paths.values()]))
    hashes = {str(p.relative_to(catalog.root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    git = subprocess.run(["git", "rev-parse", "HEAD"], cwd=catalog.root, text=True, capture_output=True, check=True).stdout.strip()
    return {"git_head": git, "files_sha256": hashes, "python": platform.python_version(), "torch": torch.__version__,
            "numpy": np.__version__, "platform": platform.platform(), "machine": platform.machine()}


def refresh_actor(learner, actor):
    """Copy a completed learner update before the next on-policy CPU rollout."""
    if actor is not learner:
        actor.load_state_dict({key: value.detach().cpu() for key, value in learner.state_dict().items()})


def train(catalog, config, config_path, output):
    if (output/"metadata.json").exists():
        raise ValueError(f"Refusing to overwrite existing experiment {output}")
    if config["device"] not in ("cpu", "cuda"):
        raise ValueError(f"Unsupported learner device in {config_path}: {config['device']}")
    if config["device"] == "cuda" and not torch.cuda.is_available():
        raise ValueError(f"run.cuda_unavailable: CUDA requested by {config_path}, but unavailable")
    if config["rollout_commands"] % config["horizon"] or config["commands"] % config["rollout_commands"]:
        raise ValueError("First pilot requires rollout and checkpoint boundaries at complete episodes")
    if any(c % config["rollout_commands"] for c in config["checkpoints"]):
        raise ValueError("Checkpoints must align with completed PPO rollouts")
    output.mkdir(parents=True, exist_ok=True)
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    torch.manual_seed(config["seed"])
    np.random.seed(config["seed"])
    random.seed(config["seed"])
    rng = np.random.default_rng(config["seed"])
    verification = verify_environment(catalog, config, output)
    env = Environment(catalog, config)
    model = Policy(env.observe(env.candidates()))
    actor = deepcopy(model) if config["device"] == "cuda" else model
    model.to(config["device"])
    optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
    metadata = {"schema_version": 1, "status": "running", "config": config, "case": catalog.case["id"],
                "parameters": model.parameter_count, "dimensions": model.dimensions, "device": config["device"],
                "actor_device": "cpu", "precision": "float32", "tf32": False,
                "cuda_runtime": torch.version.cuda,
                "gpu_name": torch.cuda.get_device_name() if config["device"] == "cuda" else None,
                "update_timing": "synchronized_stage_wall_seconds_including_transfers",
                "verification": verification, "provenance": provenance(catalog, config_path)}
    write(output/"metadata.json", metadata)
    write(output/"training-best.json", serialize(env.best))
    print(json.dumps({"event": "started", "parameters": model.parameter_count, "device": config["device"], "output": str(output)}), flush=True)
    champion_score = list(env.initial_score)
    commands, episode, optimizer_steps = 0, 0, 0
    started = time.perf_counter()
    evaluation_seconds, inference_seconds, update_seconds = 0.0, 0.0, 0.0
    snapshot_seconds = 0.0
    all_counts = Counter()

    def save_checkpoint():
        torch.save({"schema_version": 1, "commands": commands, "episode": episode,
                    "policy": model.state_dict(), "optimizer": optimizer.state_dict(),
                    "torch_rng": torch.get_rng_state(), "numpy_rng": rng.bit_generator.state,
                    "cuda_rng": torch.cuda.get_rng_state_all() if config["device"] == "cuda" else None,
                    "numpy_legacy_rng": np.random.get_state(), "python_rng": random.getstate(),
                    "config": config, "metadata": metadata, "champion_score": champion_score,
                    "optimizer_steps": optimizer_steps, "counts": dict(all_counts),
                    "training_seconds": time.perf_counter()-started-evaluation_seconds,
                    "environment_seconds": dict(env.seconds), "environment_counters": dict(env.counters)},
                   output/f"checkpoint-{commands}.pt")

    try:
        save_checkpoint()
        summary = evaluate(actor, catalog, config, output, commands, 0.0)
        evaluation_seconds += summary["evaluation_seconds"]
        from plots import render
        render(output, catalog)
        while commands < config["commands"]:
            rollout = {k: [] for k in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
            episode_return = 0.0
            for _ in range(config["rollout_commands"]):
                candidates = env.candidates()
                observation = env.observe(candidates)
                inference_started = time.perf_counter()
                action, log_prob, value = actor.act(observation)
                inference_seconds += time.perf_counter()-inference_started
                reward, done, outcome = env.step(candidates[action])
                commands += 1
                for key, item in (("observations", observation), ("actions", action), ("log_probs", log_prob),
                                  ("values", value), ("rewards", reward), ("dones", done)):
                    rollout[key].append(item)
                episode_return += reward
                if env.best_score < champion_score:
                    champion_score = list(env.best_score)
                    write(output/"training-best.json", serialize(env.best))
                    write(output/"training-best-trace.json", env.trace)
                    print(json.dumps({"event": "discovery", "commands": commands, "score": champion_score}), flush=True)
                if done:
                    episode += 1
                    row = {"episode": episode, "commands": commands, "return": episode_return,
                           "score": list(env.best_score), "counts": dict(env.episode_counts),
                           "training_champion_reduction": (env.initial_score[0]-champion_score[0])/env.initial_score[0],
                           "unfinished": not check(catalog, serialize(env.draft))["valid"]}
                    if abs(episode_return-(env.initial_score[0]-env.best_score[0])/env.initial_score[0]) > 1e-10:
                        raise RuntimeError("learning.reward_mismatch: episode return differs from outcome")
                    append(output/"episodes.jsonl", row)
                    all_counts.update(env.episode_counts)
                    env.reset()
                    episode_return = 0.0
            update_started = time.perf_counter()
            metrics = update(model, optimizer, rollout, config, rng)
            update_seconds += time.perf_counter()-update_started
            snapshot_started = time.perf_counter()
            refresh_actor(model, actor)
            snapshot_seconds += time.perf_counter()-snapshot_started
            optimizer_steps += metrics["optimizer_steps"]
            append(output/"updates.jsonl", {"commands": commands, **metrics})
            if commands % 2048 == 0:
                elapsed = time.perf_counter()-started-evaluation_seconds
                print(json.dumps({"event": "progress", "commands": commands, "episodes": episode,
                                  "training_seconds": round(elapsed, 2), "commands_per_second": round(commands/max(elapsed, .001), 2),
                                  "champion_score": champion_score, "positive_reward_commands": all_counts["positive_rewards"]}), flush=True)
            if commands in config["checkpoints"]:
                save_checkpoint()
                summary = evaluate(actor, catalog, config, output, commands, time.perf_counter()-started-evaluation_seconds)
                evaluation_seconds += summary["evaluation_seconds"]
                render(output, catalog)
        metadata.update({"status": "complete", "completed_commands": commands, "episodes": episode,
                         "optimizer_steps": optimizer_steps, "training_seconds": time.perf_counter()-started-evaluation_seconds,
                         "evaluation_seconds": evaluation_seconds, "inference_seconds": inference_seconds,
                         "update_seconds": update_seconds, "environment_seconds": dict(env.seconds),
                         "snapshot_seconds": snapshot_seconds,
                         "peak_cuda_allocated_bytes": torch.cuda.max_memory_allocated() if config["device"] == "cuda" else 0,
                         "environment_counters": dict(env.counters), "counts": dict(all_counts),
                         "peak_rss_bytes": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss*(1 if platform.system() == "Darwin" else 1024),
                         "training_champion_score": champion_score})
        write(output/"metadata.json", metadata)
        render(output, catalog)
        print(json.dumps({"event": "complete", "commands": commands, "training_seconds": metadata["training_seconds"],
                          "champion_score": champion_score, "output": str(output)}), flush=True)
    except BaseException as e:
        metadata.update({"status": "interrupted", "completed_commands": commands, "episodes": episode,
                         "error": {"code": "run.interrupted", "severity": "error", "stage": "run",
                                   "message": str(e), "entity": str(output)}})
        write(output/"metadata.json", metadata)
        # Preserve the last complete training checkpoint; partial rollout observations are
        # retained separately so an interrupted on-policy stream is not represented by a seed.
        torch.save({"commands": commands, "policy": model.state_dict(), "optimizer": optimizer.state_dict(),
                    "environment": serialize(env.draft), "base": serialize(env.base), "best": serialize(env.best),
                    "episode_step": env.steps, "torch_rng": torch.get_rng_state(), "numpy_rng": rng.bit_generator.state,
                    "cuda_rng": torch.cuda.get_rng_state_all() if config["device"] == "cuda" else None,
                    "actor_policy": actor.state_dict(),
                    "partial_rollout": locals().get("rollout"), "episode_return": locals().get("episode_return")},
                   output/"interrupted-state.pt")
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("check", "train"))
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    catalog = Catalog(args.root.resolve(), args.case.resolve())
    config = json.loads(args.config.read_text())
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.command == "check":
        print(json.dumps(verify_environment(catalog, config, output), indent=2))
    else:
        train(catalog, config, args.config, output)


if __name__ == "__main__":
    main()
