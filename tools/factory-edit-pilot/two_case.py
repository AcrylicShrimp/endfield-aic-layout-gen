"""Run a declared single/mixed-case learning condition using the owned Rust environments."""

import argparse
from collections import Counter
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import random
import time

import numpy as np
import torch

from evidence import append, provenance, write
from native import Runtime, layout, performance, snapshot, trace
from native_training_comparison import policy_hash
from policy import Policy, update
from run import evaluate, refresh_actor, verify_environment


def load_study(root, protocol_path, condition, seed, native_library):
    protocol = json.loads(protocol_path.read_text())
    if protocol.get("schema_version") != 1 or condition not in protocol.get("conditions", {}):
        raise ValueError(f"study.protocol: {protocol_path}: unsupported version or condition {condition}")
    if seed not in protocol["training_seeds"]:
        raise ValueError(f"study.seed: {protocol_path}: undeclared training seed {seed}")
    base_path = root / protocol["base_config"]
    config = json.loads(base_path.read_text())
    selected = protocol["conditions"][condition]
    schedule = selected["schedule"]
    if (not schedule or any(key not in protocol["cases"] for key in schedule)
            or len(set(schedule)) != len(schedule)):
        raise ValueError(f"study.schedule: {protocol_path}: expected a nonempty cycle of distinct declared cases")
    config.update(seed=seed, commands=selected["commands"], checkpoints=[
        count for count in protocol["checkpoints"] if count <= selected["commands"]])
    if not protocol["cases"]:
        raise ValueError(f"study.cases: {protocol_path}: at least one case is required")
    cycle = config["horizon"] * len(schedule)
    if (config["horizon"] <= 0 or config["rollout_commands"] <= 0 or config["commands"] <= 0
            or config["rollout_commands"] % cycle or config["commands"] % config["rollout_commands"]
            or config["checkpoints"] != sorted(set(config["checkpoints"]))
            or not config["checkpoints"] or config["checkpoints"][0] != 0
            or config["checkpoints"][-1] != config["commands"]
            or any(count < 0 or count % config["rollout_commands"] for count in config["checkpoints"])):
        raise ValueError(f"study.allowance: {protocol_path}: checkpoints and rollouts must contain complete schedule cycles")
    if config["device"] not in ("cpu", "cuda") or config.get("compile_actor", False):
        raise ValueError(f"study.learner: {base_path}: study requires an eager CPU actor with CPU/CUDA learner")
    if config["evaluation_seeds"] < 2:
        raise ValueError(f"study.evaluation: {base_path}: at least two evaluation rollouts required")
    if config["device"] == "cuda" and not torch.cuda.is_available():
        raise ValueError(f"study.cuda_unavailable: {base_path}: CUDA learner requested but unavailable")
    runtimes = {key: Runtime(root, root / path, config, native_library) for key, path in protocol["cases"].items()}
    if protocol["initialization_case"] not in runtimes:
        raise ValueError(f"study.initialization: {protocol_path}: unknown initialization case")
    validate_compatibility(runtimes)
    return protocol, base_path, config, runtimes


def production_signature(case):
    return {"catalogs": case["catalogs"], "target": case["target"],
            "facilities": [{key: value for key, value in row.items() if key != "pose"}
                           for row in case["facilities"]],
            "networks": [{key: value for key, value in row.items()
                          if key not in ("corners", "source_binding", "target_binding")}
                         for row in case["networks"]]}


def validate_compatibility(runtimes):
    first = next(iter(runtimes.values()))
    fields = ("engine_version", "snapshot_version", "facility_ids", "facility_types", "network_ids",
              "terminal_ids", "recipes", "operations", "observation_dimensions")
    for runtime in runtimes.values():
        if production_signature(runtime.case) != production_signature(first.case):
            raise ValueError(f"study.production: {runtime.path}: case production semantics or category order differ")
        for field in fields:
            if runtime.metadata[field] != first.metadata[field]:
                raise ValueError(f"study.features: {runtime.path}: incompatible feature meaning: {field}")


def study_provenance(runtime, protocol_path, base_path):
    result = provenance(runtime, protocol_path)
    path = base_path.resolve()
    field = "files_sha256" if path.is_relative_to(runtime.root) else "external_files_sha256"
    key = str(path.relative_to(runtime.root)) if field == "files_sha256" else str(path)
    result[field][key] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def evaluate_cases(actor, runtimes, config, output, commands, case_commands, training_seconds):
    results = {}
    for key, runtime in runtimes.items():
        directory = output / key
        summary = evaluate(actor, runtime, config, directory, commands, training_seconds)
        rows = [json.loads(line) for line in (directory / "evaluations.jsonl").read_text().splitlines()
                if line.strip()][-config["evaluation_seeds"]:]
        gains = [row["counts"].get("positive_rewards", 0) for row in rows]
        summary.update(case=key, case_commands=case_commands[key],
                       multiple_improvement_frequency=sum(count >= 2 for count in gains) / len(gains),
                       mean_positive_improvements=float(np.mean(gains)))
        for label in ("best", "median"):
            value = json.loads((directory / f"evaluation-{label}.json").read_text())
            write(directory / f"evaluation-{commands}-{label}.json", value)
            root = runtime.case["facilities"]
            summary[f"{label}_moved_facilities"] = [row["id"] for row in root
                                                    if value["placements"][row["id"]] != row["pose"]]
        append(output / "evaluations.jsonl", summary)
        results[key] = summary
    return results


def train(root, protocol_path, condition, seed, native_library, output):
    if output.exists():
        raise ValueError(f"study.output_exists: {output}: choose a fresh output directory")
    protocol, base_path, config, runtimes = load_study(root, protocol_path, condition, seed, native_library)
    schedule = protocol["conditions"][condition]["schedule"]
    torch.set_num_threads(config["threads"])
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    torch.manual_seed(seed)
    np.random.seed(seed)
    random.seed(seed)
    rng = np.random.default_rng(seed)
    output.mkdir(parents=True)
    verification, sources, environments = {}, {}, {}
    for key, runtime in runtimes.items():
        (output / key).mkdir()
        verification[key] = verify_environment(runtime, config, output / key)
        sources[key] = study_provenance(runtime, protocol_path, base_path)
        environments[key] = runtime.new_engine()
        write(output / key / "training-best.json", layout(environments[key], "root"))
        write(output / key / "training-best-trace.json", [])
    example, _ = environments[protocol["initialization_case"]].observe()
    model = Policy(example)
    initial_policy_hash = policy_hash(model.state_dict())
    actor = deepcopy(model) if config["device"] == "cuda" else model
    model.to(config["device"])
    optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
    commands, episodes, optimizer_steps = 0, 0, 0
    case_commands = {key: 0 for key in runtimes}
    case_episodes = {key: 0 for key in runtimes}
    counts = {key: Counter() for key in runtimes}
    champions = {key: list(env.initial_score) for key, env in environments.items()}
    seconds = {key: 0.0 for key in ("evaluation", "inference", "update", "actor_snapshot")}
    started = time.perf_counter()
    phase, active_case, episode_return, rollout = "setup", schedule[0], 0.0, None

    def metadata(status, error=None):
        value = {"schema_version": 1, "experiment_family": "two_case", "study": protocol["id"],
                 "status": status, "condition": condition, "schedule": schedule, "config": config,
                 "protocol_path": str(protocol_path), "base_config_path": str(base_path),
                 "parameters": model.parameter_count, "dimensions": model.dimensions,
                 "initial_policy_sha256": initial_policy_hash, "device": config["device"],
                 "actor_device": "cpu", "environment_backend": "rust", "precision": "float32", "tf32": False,
                 "completed_commands": commands, "episodes": episodes, "optimizer_steps": optimizer_steps,
                 "case_commands": case_commands, "case_episodes": case_episodes,
                 "counts": {key: dict(value) for key, value in counts.items()}, "champions": champions,
                 "training_seconds": time.perf_counter() - started - seconds["evaluation"],
                 "seconds": dict(seconds), "provenance": sources, "verification": verification,
                 "environment_performance": {key: performance(env) for key, env in environments.items()}}
        if error is not None:
            value["error"] = error
        return value

    def record_metadata(status, error=None):
        value = metadata(status, error)
        write(output / "metadata.json", value)
        for key in runtimes:
            write(output / key / "metadata.json", {**value, "case": key, "provenance": sources[key],
                  "verification": verification[key], "training_champion_score": champions[key]})
        return value

    def save_checkpoint(kind, path):
        value = {"schema_version": 1, "checkpoint_kind": kind, "phase": phase, "metadata": metadata("running"),
                 "commands": commands, "episodes": episodes, "optimizer_steps": optimizer_steps,
                 "case_commands": dict(case_commands), "case_episodes": dict(case_episodes),
                 "counts": {key: dict(value) for key, value in counts.items()}, "champions": deepcopy(champions),
                 "next_schedule_index": episodes % len(schedule), "active_case": active_case,
                 "episode_return": episode_return, "environments": {key: snapshot(env) for key, env in environments.items()},
                 "policy": model.state_dict(), "actor_policy": actor.state_dict(), "optimizer": optimizer.state_dict(),
                 "torch_rng": torch.get_rng_state(), "numpy_rng": rng.bit_generator.state,
                 "numpy_legacy_rng": np.random.get_state(), "python_rng": random.getstate(),
                 "cuda_rng": torch.cuda.get_rng_state_all() if config["device"] == "cuda" else None,
                 "partial_rollout": rollout if kind == "two_case_interrupted" else None}
        temporary = path.with_suffix(".tmp")
        torch.save(value, temporary)
        temporary.replace(path)

    def checkpoint_and_evaluate():
        nonlocal phase
        phase = "checkpoint"
        save_checkpoint("two_case_complete_rollout", output / f"checkpoint-{commands}.pt")
        phase = "evaluation"
        clock = time.perf_counter()
        try:
            evaluate_cases(actor, runtimes, config, output, commands, case_commands,
                           clock - started - seconds["evaluation"])
        finally:
            seconds["evaluation"] += time.perf_counter() - clock
        record_metadata("running")

    record_metadata("running")
    print(json.dumps({"event": "study_started", "condition": condition, "seed": seed,
                      "commands": config["commands"], "initial_policy_sha256": initial_policy_hash}), flush=True)
    try:
        checkpoint_and_evaluate()
        while commands < config["commands"]:
            phase = "collection"
            rollout = {key: [] for key in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
            for _ in range(config["rollout_commands"]):
                active_case = schedule[episodes % len(schedule)]
                env = environments[active_case]
                observation, handle = env.observe()
                clock = time.perf_counter()
                action, log_prob, value = actor.act(observation)
                seconds["inference"] += time.perf_counter() - clock
                reward, done, outcome = env.step(handle, action)
                commands += 1
                case_commands[active_case] += 1
                episode_return += reward
                for key, item in (("observations", observation), ("actions", action), ("log_probs", log_prob),
                                  ("values", value), ("rewards", reward), ("dones", done)):
                    rollout[key].append(item)
                if outcome["improved"] and env.best_score < champions[active_case]:
                    champions[active_case] = list(env.best_score)
                    write(output / active_case / "training-best.json", layout(env, "best"))
                    write(output / active_case / "training-best-trace.json", trace(env))
                if done:
                    episodes += 1
                    case_episodes[active_case] += 1
                    stats = json.loads(env.episode_json())
                    if abs(episode_return - (env.initial_score[0] - env.best_score[0]) / env.initial_score[0]) > 1e-10:
                        raise RuntimeError(f"study.reward: {active_case}: episode return does not match retained quality")
                    row = {"case": active_case, "episode": episodes, "case_episode": case_episodes[active_case],
                           "commands": commands, "case_commands": case_commands[active_case],
                           "return": episode_return, "score": stats["best_score"], "counts": stats["counts"],
                           "training_champion_reduction": (env.initial_score[0] - champions[active_case][0]) / env.initial_score[0],
                           "unfinished": not stats["draft_acceptance"]["valid"]}
                    append(output / "episodes.jsonl", row)
                    append(output / active_case / "episodes.jsonl", row)
                    counts[active_case].update(stats["counts"])
                    env.reset()
                    episode_return = 0.0
            phase = "update"
            clock = time.perf_counter()
            metrics = update(model, optimizer, rollout, config, rng)
            seconds["update"] += time.perf_counter() - clock
            optimizer_steps += metrics["optimizer_steps"]
            append(output / "updates.jsonl", {"commands": commands, "case_commands": dict(case_commands), **metrics})
            phase = "actor_snapshot"
            clock = time.perf_counter()
            refresh_actor(model, actor)
            seconds["actor_snapshot"] += time.perf_counter() - clock
            if commands % 8192 == 0:
                print(json.dumps({"event": "study_progress", "condition": condition, "seed": seed,
                                  "commands": commands, "case_commands": case_commands, "champions": champions,
                                  "training_seconds": round(time.perf_counter() - started - seconds["evaluation"], 3)}), flush=True)
            if commands in config["checkpoints"]:
                checkpoint_and_evaluate()
        phase = "render"
        record_metadata("running")
        from plots import render
        for key, runtime in runtimes.items():
            render(output / key, runtime)
        result = record_metadata("complete")
        print(json.dumps({"event": "study_complete", "condition": condition, "seed": seed,
                          "commands": commands, "case_commands": case_commands, "champions": champions,
                          "training_seconds": result["training_seconds"]}), flush=True)
        return result
    except BaseException as error:
        diagnostic = {"code": "study.interrupted", "severity": "error", "stage": f"study.{phase}",
                      "message": str(error) or type(error).__name__, "entity": str(output)}
        record_metadata("interrupted", diagnostic)
        save_checkpoint("two_case_interrupted", output / "interrupted-state.pt")
        raise


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
