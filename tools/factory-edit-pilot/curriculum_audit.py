"""Independent source, curriculum exposure, checkpoint and original-root evidence audit."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import numpy as np
import torch

from audit import audit
from curriculum import remaining_at
from evidence import write
from native_training_comparison import policy_hash, rows
from reference.acceptance import Catalog, check
from reference.environment import Environment


def audit_study(root, protocol_path, output, source_commit):
    protocol = json.loads(protocol_path.read_text())
    metadata = json.loads((output / "metadata.json").read_text())
    condition, config = metadata["condition"], metadata["config"]
    expected = json.loads((root / protocol["base_config"]).read_text())
    expected.update(seed=metadata["seed"], commands=protocol["commands"], checkpoints=protocol["checkpoints"],
                    rollout_commands=protocol["streams"] * expected["horizon"])
    if (metadata.get("experiment_family") != "curriculum_parallel" or metadata["status"] != "complete"
            or metadata["protocol"] != protocol or config != expected
            or metadata["seed"] not in protocol["training_seeds"]):
        raise ValueError(f"curriculum_audit.protocol: {output}: incomplete run or configuration mismatch")
    starts = json.loads((output / "starts.json").read_text())
    for key, path in protocol["cases"].items():
        env = Environment(Catalog(root, root / path), config)
        witness = env.catalog.case["witness"]
        first_gain = None
        for i, command in enumerate(witness):
            if env.step(command)[0] > 0:
                first_gain = i + 1
                break
        if first_gain is None or starts[key] != {str(n): witness[:first_gain-n] if n else [] for n in range(first_gain + 1)}:
            raise ValueError(f"curriculum_audit.starts: {output}: {key}: starts differ from declared first-success prefixes")
    episodes = rows(output / "episodes.jsonl")
    exposure, stages, setup = Counter(), Counter(), 0
    counters = {key: Counter() for key in protocol["cases"]}
    for i, row in enumerate(episodes):
        key = list(protocol["cases"])[i % len(protocol["cases"])]
        before = i * config["horizon"]
        remaining = remaining_at(protocol, condition, before)
        size = len(starts[key][str(remaining)])
        if (row["case"] != key or row["episode"] != i + 1 or row["commands"] != before + config["horizon"]
                or row["remaining"] != remaining or row["setup_commands"] != size
                or row["counts"]["commands"] != config["horizon"]):
            raise ValueError(f"curriculum_audit.episode: {output}: episode {i+1}: schedule or exposure differs")
        exposure[key] += config["horizon"]
        stages[f"{key}:{remaining}"] += config["horizon"]
        counters[key].update(row["counts"])
        setup += size
    updates = rows(output / "updates.jsonl")
    steps = config["epochs"] * ((config["rollout_commands"] - 1) // config["minibatch"] + 1)
    if ([row["commands"] for row in updates] != list(range(config["rollout_commands"], config["commands"] + 1, config["rollout_commands"]))
            or any(row["optimizer_steps"] != steps or row["remaining"] != remaining_at(protocol, condition, row["commands"] - config["rollout_commands"])
                   for row in updates)
            or metadata["optimizer_steps"] != len(updates) * steps
            or metadata["completed_commands"] != config["commands"]
            or len(episodes) * config["horizon"] != config["commands"] or metadata["episodes"] != len(episodes)
            or metadata["case_commands"] != dict(exposure) or metadata["stage_commands"] != dict(stages)
            or metadata["setup_commands"] != setup
            or metadata["counts"] != {key: dict(value) for key, value in counters.items()}):
        raise ValueError(f"curriculum_audit.accounting: {output}: sample, setup, stage or optimizer totals differ")
    hashes = {}
    for count in config["checkpoints"]:
        path = output / f"checkpoint-{count}.pt"
        saved = torch.load(path, map_location="cpu", weights_only=False)
        m = saved["metadata"]
        if (saved["kind"] != "complete_rollout" or m["completed_commands"] != count
                or m["optimizer_steps"] != count // config["rollout_commands"] * steps
                or len(saved["environments"]) != protocol["streams"] or len(saved["stream_rngs"]) != protocol["streams"]
                or any(state["steps"] != (config["horizon"] if count else 0) for state in saved["environments"])
                or policy_hash(saved["policy"]) != policy_hash(saved["evaluation_policy"])):
            raise ValueError(f"curriculum_audit.checkpoint: {path}: inconsistent policy or completed exposure")
        if count == 0 and policy_hash(saved["policy"]) != metadata["initial_policy_sha256"]:
            raise ValueError(f"curriculum_audit.initialization: {path}: initial model hash differs")
        hashes[str(count)] = hashlib.sha256(path.read_bytes()).hexdigest()
    summaries = rows(output / "evaluations.jsonl")
    if [(row["commands"], row["case"]) for row in summaries] != [(count, key) for count in config["checkpoints"] for key in protocol["cases"]]:
        raise ValueError(f"curriculum_audit.evaluation_matrix: {output}: missing or duplicate evaluations")
    result = {"schema_version": 1, "condition": condition, "seed": metadata["seed"], "source_commit": source_commit,
              "initial_policy_sha256": metadata["initial_policy_sha256"], "case_commands": dict(exposure),
              "stage_commands": dict(stages), "setup_commands": setup, "checkpoints_sha256": hashes, "cases": {}}
    for key, path in protocol["cases"].items():
        directory = output / key
        case_metadata = json.loads((directory / "metadata.json").read_text())
        if (case_metadata["provenance"] != metadata["provenance"][key] or case_metadata["config"] != config
                or not case_metadata["provenance"]["git_worktree_clean"]
                or case_metadata["provenance"]["git_head"] != source_commit):
            raise ValueError(f"curriculum_audit.provenance: {directory}: inconsistent or uncommitted source")
        checked = audit(root, root / path, directory, source_commit)
        result["cases"][key] = checked
        initial = checked["layouts"]["initial.json"]
        subset = [row for row in episodes if row["case"] == key]
        for row in subset:
            if abs(row["return"] - (initial[0] - row["score"][0]) / initial[0]) > 1e-10:
                raise ValueError(f"curriculum_audit.reward: {directory}: training return differs from retained area")
        champion = min([initial, *[row["score"] for row in subset]])
        if (champion != metadata["champions"][key] or champion != checked["layouts"]["training-best.json"]
                or champion != checked["traces"]["training-best-trace.json"]["score"]):
            raise ValueError(f"curriculum_audit.champion: {directory}: selected training layout or replay differs")
        start_path = directory / "training-best-start.json"
        if start_path.exists():
            selected = json.loads(start_path.read_text())
            episode = next(row for row in subset if row["episode"] == selected["episode"])
            if (selected != {"prefix": starts[key][str(episode["remaining"])], **episode}
                    or selected["score"] != champion):
                raise ValueError(f"curriculum_audit.champion_start: {start_path}: start differs from selected training episode")
        elif champion != initial:
            raise ValueError(f"curriculum_audit.champion_start: {directory}: missing selected start")
        evaluations = rows(directory / "evaluations.jsonl")
        if [(row["commands"], row["rollout_seed"]) for row in evaluations] != [
                (count, config["evaluation_seed_start"] + i) for count in config["checkpoints"] for i in range(config["evaluation_seeds"])]:
            raise ValueError(f"curriculum_audit.evaluation_seeds: {directory}: frozen seed schedule differs")
        catalog = Catalog(root, root / path)
        for count in config["checkpoints"]:
            selected = [row for row in evaluations if row["commands"] == count]
            summary = next(row for row in summaries if row["case"] == key and row["commands"] == count)
            values = np.asarray([row["return"] for row in selected])
            expected = {"mean_return": float(values.mean()), "median_return": float(np.median(values)),
                        "max_return": float(values.max()), "improvement_frequency": float((values > 0).mean()),
                        "multiple_improvement_frequency": sum(row["counts"].get("positive_rewards", 0) >= 2 for row in selected) / len(selected)}
            if any(abs(summary[field] - value) > 1e-12 for field, value in expected.items()):
                raise ValueError(f"curriculum_audit.summary: {directory}: {count}: aggregate differs from rows")
            for row in selected:
                if row["counts"]["commands"] != config["horizon"] or abs(row["return"] - (initial[0] - row["score"][0]) / initial[0]) > 1e-10:
                    raise ValueError(f"curriculum_audit.evaluation_reward: {directory}: {count}: reward or horizon differs")
            ordered = sorted(selected, key=lambda row: row["score"])
            for label, row in (("best", ordered[0]), ("median", ordered[len(ordered)//2])):
                accepted = check(catalog, json.loads((directory / f"evaluation-{count}-{label}.json").read_text()))
                if not accepted["valid"] or accepted["score"] != row["score"]:
                    raise ValueError(f"curriculum_audit.selected: {directory}: {count}: invalid or mismatched selected layout")
            if checked["traces"][f"evaluation-{count}-representative-trace.json"]["score"] != ordered[len(ordered)//2]["score"]:
                raise ValueError(f"curriculum_audit.replay: {directory}: {count}: representative trace differs")
    result.update(status="passed", metadata_sha256=hashlib.sha256((output / "metadata.json").read_bytes()).hexdigest())
    write(output / "study-audit.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    args = parser.parse_args()
    print(json.dumps(audit_study(args.root.resolve(), args.protocol.resolve(), args.output.resolve(), args.source_commit), indent=2))


if __name__ == "__main__":
    main()
