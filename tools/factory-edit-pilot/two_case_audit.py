"""Independently audit a completed two-case condition and its declared learning exposure."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import numpy as np
import torch

from audit import audit
from evidence import write
from native_training_comparison import policy_hash, rows
from reference.acceptance import Catalog, check


def audit_study(root, protocol_path, output, source_commit):
    protocol = json.loads(protocol_path.read_text())
    metadata = json.loads((output / "metadata.json").read_text())
    if metadata.get("experiment_family") != "two_case" or metadata["status"] != "complete":
        raise ValueError(f"study_audit.incomplete: {output}: expected a completed two-case run")
    condition, config = metadata["condition"], metadata["config"]
    spec = protocol["conditions"][condition]
    expected_config = json.loads((root / protocol["base_config"]).read_text())
    expected_config.update(seed=config["seed"], commands=spec["commands"], checkpoints=[
        count for count in protocol["checkpoints"] if count <= spec["commands"]])
    if (config != expected_config or config["seed"] not in protocol["training_seeds"]
            or metadata["schedule"] != spec["schedule"] or metadata["study"] != protocol["id"]):
        raise ValueError(f"study_audit.protocol: {output}: recorded configuration differs from protocol")
    if metadata["completed_commands"] != spec["commands"]:
        raise ValueError(f"study_audit.allowance: {output}: training allowance not completed")
    epochs = rows(output / "episodes.jsonl")
    case_commands = {key: 0 for key in protocol["cases"]}
    case_episodes = {key: 0 for key in protocol["cases"]}
    counters = {key: Counter() for key in protocol["cases"]}
    for index, row in enumerate(epochs):
        key = spec["schedule"][index % len(spec["schedule"])]
        case_commands[key] += config["horizon"]
        case_episodes[key] += 1
        if (row["case"] != key or row["commands"] != (index + 1) * config["horizon"]
                or row["case_commands"] != case_commands[key] or row["case_episode"] != case_episodes[key]
                or row["episode"] != index + 1 or row["counts"]["commands"] != config["horizon"]):
            raise ValueError(f"study_audit.schedule: {output}: episode {index + 1} differs from declared exposure")
        counters[key].update(row["counts"])
    updates = rows(output / "updates.jsonl")
    steps_per_update = config["epochs"] * ((config["rollout_commands"] - 1) // config["minibatch"] + 1)
    if ([row["commands"] for row in updates] != list(range(config["rollout_commands"], spec["commands"] + 1,
                                                         config["rollout_commands"]))
            or sum(row["optimizer_steps"] for row in updates) != metadata["optimizer_steps"]
            or any(row["optimizer_steps"] != steps_per_update for row in updates)
            or len(epochs) * config["horizon"] != spec["commands"]
            or metadata["case_commands"] != case_commands or metadata["case_episodes"] != case_episodes
            or metadata["episodes"] != len(epochs)
            or metadata["counts"] != {key: dict(value) for key, value in counters.items()}):
        raise ValueError(f"study_audit.accounting: {output}: episode/update totals differ")
    initial = torch.load(output / "checkpoint-0.pt", map_location="cpu", weights_only=False)
    actual_initial_hash = policy_hash(initial["policy"])
    if actual_initial_hash != metadata["initial_policy_sha256"]:
        raise ValueError(f"study_audit.initialization: {output}: initial parameter hash differs")
    for count in config["checkpoints"]:
        saved = torch.load(output / f"checkpoint-{count}.pt", map_location="cpu", weights_only=False)
        expected = {key: 0 for key in protocol["cases"]}
        for index in range(count // config["horizon"]):
            expected[spec["schedule"][index % len(spec["schedule"])]] += config["horizon"]
        if (saved["checkpoint_kind"] != "two_case_complete_rollout" or saved["commands"] != count
                or saved["case_commands"] != expected or any(state["steps"] for state in saved["environments"].values())):
            raise ValueError(f"study_audit.checkpoint: {output}: inconsistent completed checkpoint {count}")
    summaries = rows(output / "evaluations.jsonl")
    if [(row["commands"], row["case"]) for row in summaries] != [
            (count, key) for count in config["checkpoints"] for key in protocol["cases"]]:
        raise ValueError(f"study_audit.evaluations: {output}: case/checkpoint evaluation matrix differs")
    result = {"schema_version": 1, "condition": condition, "seed": config["seed"], "source_commit": source_commit,
              "case_commands": case_commands, "initial_policy_sha256": actual_initial_hash, "cases": {}}
    for key, path in protocol["cases"].items():
        directory = output / key
        case_metadata = json.loads((directory / "metadata.json").read_text())
        if (case_metadata["provenance"] != metadata["provenance"][key] or case_metadata["config"] != config
                or case_metadata["verification"] != metadata["verification"][key]):
            raise ValueError(f"study_audit.case_metadata: {directory}: differs from the parent run")
        result["cases"][key] = audit(root, (root / path).resolve(), directory, source_commit)
        catalog = Catalog(root, root / path)
        initial_area = result["cases"][key]["layouts"]["initial.json"][0]
        case_rows = [row for row in epochs if row["case"] == key]
        recorded_case_rows = rows(directory / "episodes.jsonl") if (directory / "episodes.jsonl").exists() else []
        if recorded_case_rows != case_rows:
            raise ValueError(f"study_audit.case_episodes: {directory}: differs from the parent episode log")
        for row in case_rows:
            if abs(row["return"] - (initial_area - row["score"][0]) / initial_area) > 1e-10:
                raise ValueError(f"study_audit.reward: {directory}: episode return differs from score")
        champion = min([result["cases"][key]["layouts"]["initial.json"], *[row["score"] for row in case_rows]])
        if champion != metadata["champions"][key] or champion != result["cases"][key]["layouts"]["training-best.json"]:
            raise ValueError(f"study_audit.champion: {directory}: retained champion differs from episode results")
        if result["cases"][key]["traces"]["training-best-trace.json"]["score"] != champion:
            raise ValueError(f"study_audit.champion_trace: {directory}: replay does not reproduce champion")
        evaluations = rows(directory / "evaluations.jsonl")
        expected_keys = [(count, config["evaluation_seed_start"] + i)
                         for count in config["checkpoints"] for i in range(config["evaluation_seeds"])]
        if [(row["commands"], row["rollout_seed"]) for row in evaluations] != expected_keys:
            raise ValueError(f"study_audit.evaluation_seeds: {directory}: frozen sampling schedule differs")
        for count in config["checkpoints"]:
            subset = [row for row in evaluations if row["commands"] == count]
            summary = next(row for row in summaries if row["case"] == key and row["commands"] == count)
            values = np.asarray([row["return"] for row in subset])
            expected = {"mean_return": float(values.mean()), "median_return": float(np.median(values)),
                        "max_return": float(values.max()), "improvement_frequency": float((values > 0).mean()),
                        "multiple_improvement_frequency": sum(row["counts"].get("positive_rewards", 0) >= 2
                                                              for row in subset) / len(subset)}
            if any(abs(summary[field] - value) > 1e-12 for field, value in expected.items()):
                raise ValueError(f"study_audit.summary: {directory}: {count}: differs from recorded evaluation rows")
            for row in subset:
                if (row["counts"]["commands"] != config["horizon"]
                        or abs(row["return"] - (initial_area - row["score"][0]) / initial_area) > 1e-10):
                    raise ValueError(f"study_audit.evaluation_reward: {directory}: {count}: invalid allowance or reward")
            ordered = sorted(subset, key=lambda row: row["score"])
            for label, selected in (("best", ordered[0]), ("median", ordered[len(ordered) // 2])):
                name = f"evaluation-{count}-{label}.json"
                accepted = check(catalog, json.loads((directory / name).read_text()))
                if not accepted["valid"] or accepted["score"] != selected["score"]:
                    raise ValueError(f"study_audit.selected_layout: {directory / name}: invalid or wrong score")
            replayed = result["cases"][key]["traces"][f"evaluation-{count}-representative-trace.json"]
            if replayed["score"] != ordered[len(ordered) // 2]["score"]:
                raise ValueError(f"study_audit.selected_trace: {directory}: {count}: does not reproduce median")
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
