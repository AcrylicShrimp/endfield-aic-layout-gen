"""Reproduce every positive original-root evaluation from its frozen policy checkpoint."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import torch

from evidence import write
from native import Runtime, layout, trace
from native_training_comparison import policy_hash, rows
from policy import Policy
from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize


def replay(root, run_path, library, output):
    if output.exists():
        raise ValueError(f"evaluation_replay.output_exists: {output}")
    metadata = json.loads((run_path / "metadata.json").read_text())
    if metadata["status"] != "complete" or metadata["experiment_family"] not in ("curriculum_parallel", "mastery_curriculum"):
        raise ValueError(f"evaluation_replay.incomplete: {run_path}")
    config = metadata["config"]
    torch.set_num_threads(config["threads"])
    output.mkdir(parents=True)
    results = []
    for key, case_path in metadata["protocol"]["cases"].items():
        runtime = Runtime(root, root / case_path, config, library)
        for recorded in rows(run_path / key / "evaluations.jsonl"):
            if recorded["return"] <= 0:
                continue
            count, seed = recorded["commands"], recorded["rollout_seed"]
            checkpoint_path = run_path / f"checkpoint-{count}.pt"
            saved = torch.load(checkpoint_path, map_location="cpu", weights_only=False)
            env = runtime.new_engine()
            model = Policy(env.observe()[0], normalization=config.get("normalization", "none"),
                       width_multiplier=config.get("width_multiplier", 1))
            model.load_state_dict(saved["evaluation_policy"])
            model.eval()
            generator = torch.Generator(device="cpu").manual_seed(seed)
            total = 0.0
            for _ in range(config["horizon"]):
                observation, handle = env.observe()
                action, _, _ = model.act(observation, generator)
                reward, _, _ = env.step(handle, action)
                total += reward
            native_trace = trace(env)
            stats = json.loads(env.episode_json())
            if (total != recorded["return"] or list(env.best_score) != recorded["score"]
                    or stats["counts"] != recorded["counts"]):
                raise RuntimeError(f"evaluation_replay.policy: {key}/{count}/{seed}: frozen evaluation does not reproduce")
            reference = Environment(Catalog(root, root / case_path), config)
            for i, row in enumerate(native_trace):
                if row["command"] not in reference.candidates().to_dicts():
                    raise RuntimeError(f"evaluation_replay.domain: {key}/{count}/{seed}/{i}")
                reward, _, outcome = reference.step(row["command"])
                if reward != row["reward"] or any(outcome[field] != row[field] for field in outcome):
                    raise RuntimeError(f"evaluation_replay.transition: {key}/{count}/{seed}/{i}")
            selected = layout(env, "best")
            accepted = check(reference.catalog, selected)
            if not accepted["valid"] or serialize(reference.best) != selected:
                raise RuntimeError(f"evaluation_replay.layout: {key}/{count}/{seed}")
            name = f"{key}-{count}-seed{seed}"
            write(output / f"{name}-layout.json", selected)
            write(output / f"{name}-trace.json", native_trace)
            results.append({"case": key, **recorded, "checkpoint_sha256": hashlib.sha256(checkpoint_path.read_bytes()).hexdigest(),
                            "policy_sha256": policy_hash(saved["evaluation_policy"]),
                            "positive_steps": [i+1 for i, row in enumerate(native_trace) if row["reward"] > 0],
                            "layout_file": f"{name}-layout.json", "trace_file": f"{name}-trace.json"})
    result = {"schema_version": 1, "status": "passed", "run": str(run_path), "condition": metadata["condition"],
              "training_seed": metadata["seed"], "origin": "Original root; no curriculum prefix or warm-start",
              "checks": "Frozen CPU policy, sampling seed, return, counters and score reproduce; independent reference replays every transition and final geometry",
              "results": results, "helper_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "metadata_sha256": hashlib.sha256((run_path / "metadata.json").read_bytes()).hexdigest(),
              "git_head": subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, text=True, capture_output=True, check=True).stdout.strip()}
    write(output / "replay.json", result)
    print(json.dumps(result, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--run", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    replay(args.root.resolve(), args.run.resolve(), args.native_library.resolve(), args.output.resolve())


if __name__ == "__main__":
    main()
