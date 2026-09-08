"""Audit short/mastery exposure and independently replay every mastery assessment."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import numpy as np
import torch

from audit import audit
from policy import Policy
from reference.environment import serialize
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
                    rollout_commands=protocol["rollout_commands"])
    training = {**expected, "horizon": protocol["conditions"][condition]["horizon"]}
    if (metadata.get("experiment_family") != "mastery_curriculum" or metadata["status"] != "complete"
            or metadata["protocol"] != protocol or config != expected or metadata["training_config"] != training
            or metadata["seed"] not in protocol["training_seeds"]):
        raise ValueError(f"mastery_audit.protocol: {output}: incomplete run or configuration mismatch")
    torch.set_num_threads(config["threads"])
    starts = json.loads((output / "starts.json").read_text())
    catalogs = {key: Catalog(root, root / path) for key, path in protocol["cases"].items()}
    for key, catalog in catalogs.items():
        env = Environment(catalog, config)
        witness = catalog.case["witness"]
        first_gain = None
        for i, command in enumerate(witness):
            if env.step(command)[0] > 0:
                first_gain = i + 1
                break
        if first_gain is None or starts[key] != {str(n): witness[:first_gain-n] if n else [] for n in range(first_gain + 1)}:
            raise ValueError(f"mastery_audit.starts: {output}: {key}: unexpected setup prefix")

    def fixed_level(commands):
        return next((s["remaining"] for s in protocol["fixed_curriculum"] if commands < s["until"]), 0)

    gate_spec = protocol["assessment"]
    gates = rows(output / "assessments.jsonl")
    if [g["commands"] for g in gates] != list(range(gate_spec["interval"], config["commands"]+1, gate_spec["interval"])):
        raise ValueError(f"mastery_audit.assessments: {output}: wrong assessment cadence")
    adaptive = protocol["conditions"][condition]["advancement"] == "mastery"
    controller = {"level": 1, "streak": 0}
    levels, controllers = {}, {0: dict(controller) if adaptive else None}
    for gate in gates:
        count = gate["commands"]
        level = controller["level"] if adaptive else fixed_level(count-1)
        levels[count // gate_spec["interval"] - 1] = level
        assessed = level or 1
        if gate["training_level"] != level or gate["level"] != assessed:
            raise ValueError(f"mastery_audit.level: {output}: {count}: wrong assessed stage")
        saved = torch.load(output / f"checkpoint-{count}.pt", map_location="cpu", weights_only=False)
        example_env = Environment(next(iter(catalogs.values())), config)
        actor = Policy(example_env.observe(example_env.candidates()), normalization=config.get("normalization", "none"),
                       width_multiplier=config.get("width_multiplier", 1))
        actor.load_state_dict(saved["evaluation_policy"])
        actor.eval()
        expected_rows, successes = [], {}
        for key, catalog in catalogs.items():
            env = Environment(catalog, {**config, "horizon": gate_spec["horizon"]})
            wins = 0
            for index in range(gate_spec["episodes"]):
                env.reset()
                for command in starts[key][str(assessed)]:
                    if command not in env.candidates().to_dicts() or env.step(command)[0] != 0:
                        raise ValueError(f"mastery_audit.prefix: {output}: invalid assessment setup")
                env.steps, env.revision, env.trace, env.episode_counts = 0, 0, [], Counter()
                seed = gate_spec["seed_start"] + index
                generator = torch.Generator(device="cpu").manual_seed(seed)
                total = 0.0
                for _ in range(gate_spec["horizon"]):
                    domain = env.candidates()
                    action, _, _ = actor.act(env.observe(domain), generator)
                    total += env.step(domain.to_dicts()[action])[0]
                accepted = check(catalog, serialize(env.best))
                if not accepted["valid"]:
                    raise ValueError(f"mastery_audit.layout: {output}: assessment replay is invalid")
                wins += total > 0
                expected_rows.append({"case": key, "seed": seed, "level": assessed, "return": total,
                                      "score": accepted["score"], "counts": dict(env.episode_counts)})
            successes[key] = wins
        if gate["rows"] != expected_rows or gate["successes"] != successes:
            raise ValueError(f"mastery_audit.replay: {output}: {count}: native assessment differs from independent reference")
        if adaptive:
            before = dict(controller)
            passed = all(n >= gate_spec["required_successes"] for n in successes.values())
            if controller["level"]:
                controller["streak"] = controller["streak"] + 1 if passed else 0
                if controller["streak"] == gate_spec["consecutive_passes"]:
                    controller = {"level": controller["level"]+1 if controller["level"]<3 else 0, "streak":0}
            if gate["transition"] != {"before": before, "passed": passed, "after": controller}:
                raise ValueError(f"mastery_audit.transition: {output}: {count}: incorrect mastery advancement")
        elif gate["transition"] is not None:
            raise ValueError(f"mastery_audit.fixed_transition: {output}: unexpected mastery transition")
        controllers[count] = dict(controller) if adaptive else None
    if metadata["controller"] != (controller if adaptive else None):
        raise ValueError(f"mastery_audit.final_controller: {output}: incorrect final controller state")
    episodes = rows(output / "episodes.jsonl")
    exposure, stages, setup = Counter(), Counter(), 0
    counters = {key: Counter() for key in protocol["cases"]}
    for i, row in enumerate(episodes):
        key = list(protocol["cases"])[i % len(protocol["cases"])]
        before = i * training["horizon"]
        level = levels[before // gate_spec["interval"]]
        size = len(starts[key][str(level)])
        if (row["case"] != key or row["episode"] != i+1 or row["commands"] != before+training["horizon"]
                or row["remaining"] != level or row["horizon"] != training["horizon"] or row["setup_commands"] != size
                or row["counts"]["commands"] != training["horizon"]):
            raise ValueError(f"mastery_audit.episode: {output}: episode {i+1}: wrong stage or exposure")
        exposure[key] += training["horizon"]
        stages[f"{key}:{level}"] += training["horizon"]
        counters[key].update(row["counts"])
        setup += size
    updates = rows(output / "updates.jsonl")
    steps = config["epochs"] * ((config["rollout_commands"]-1)//config["minibatch"]+1)
    normalization = config.get("normalization", "none")
    calibration_batches = (config["rollout_commands"]-1)//config["minibatch"]+1
    if normalization == "batch":
        for row in updates:
            calibration = row.get("normalization_calibration", {})
            if (calibration.get("kind") != "batch" or calibration.get("observations") != config["rollout_commands"]
                    or calibration.get("batches") != calibration_batches or calibration.get("seconds", -1) < 0):
                raise ValueError(f"mastery_audit.normalization: {output}: inconsistent calibration exposure")
    if ([u["commands"] for u in updates] != list(range(config["rollout_commands"], config["commands"]+1, config["rollout_commands"]))
            or any(u["optimizer_steps"] != steps or u["remaining"] != levels[(u["commands"]-1)//gate_spec["interval"]] for u in updates)
            or metadata["optimizer_steps"] != len(updates)*steps or metadata["completed_commands"] != config["commands"]
            or len(episodes)*training["horizon"] != config["commands"] or metadata["episodes"] != len(episodes)
            or metadata["case_commands"] != dict(exposure) or metadata["stage_commands"] != dict(stages)
            or metadata["setup_commands"] != setup or metadata["counts"] != {k:dict(v) for k,v in counters.items()}):
        raise ValueError(f"mastery_audit.accounting: {output}: wrong sample, reset, setup or optimizer counts")
    hashes = {}
    for count in sorted(set(config["checkpoints"]) | {g["commands"] for g in gates}):
        path = output / f"checkpoint-{count}.pt"
        saved = torch.load(path, map_location="cpu", weights_only=False)
        if normalization == "batch":
            expected_batches = count//config["rollout_commands"] * calibration_batches
            if any(saved["policy"][f"normalizations.{i}.num_batches_tracked"].item() != expected_batches for i in range(3)):
                raise ValueError(f"mastery_audit.normalization: {path}: inconsistent saved running statistics")
        m = saved["metadata"]
        expected_controller = controllers[max(n for n in controllers if n <= count)]
        if (saved["kind"] != "complete_rollout" or m["completed_commands"] != count
                or m["controller"] != expected_controller
                or m["optimizer_steps"] != count//config["rollout_commands"]*steps
                or len(saved["environments"]) != protocol["streams"] or len(saved["stream_rngs"]) != protocol["streams"]
                or any(s["steps"] != (training["horizon"] if count else 0) for s in saved["environments"])
                or policy_hash(saved["policy"]) != policy_hash(saved["evaluation_policy"])):
            raise ValueError(f"mastery_audit.checkpoint: {path}: incorrect completed checkpoint")
        if count == 0 and policy_hash(saved["policy"]) != metadata["initial_policy_sha256"]:
            raise ValueError(f"mastery_audit.initialization: {path}: initial model mismatch")
        hashes[str(count)] = hashlib.sha256(path.read_bytes()).hexdigest()
    summaries = rows(output / "evaluations.jsonl")
    if [(row["commands"], row["case"]) for row in summaries] != [(count, key) for count in config["checkpoints"] for key in protocol["cases"]]:
        raise ValueError(f"mastery_audit.evaluation_matrix: {output}: missing or duplicate evaluations")
    result = {"schema_version": 1, "condition": condition, "seed": metadata["seed"], "source_commit": source_commit,
              "assessment_replay": "All native assessment rows reproduced by the independent Python environment",
              "training_horizon": training["horizon"], "controller": metadata["controller"],
              "initial_policy_sha256": metadata["initial_policy_sha256"], "case_commands": dict(exposure),
              "stage_commands": dict(stages), "setup_commands": setup, "checkpoints_sha256": hashes, "cases": {}}
    for key, path in protocol["cases"].items():
        directory = output / key
        case_metadata = json.loads((directory / "metadata.json").read_text())
        if (case_metadata["provenance"] != metadata["provenance"][key] or case_metadata["config"] != config or case_metadata["training_config"] != training
                or not case_metadata["provenance"]["git_worktree_clean"]
                or case_metadata["provenance"]["git_head"] != source_commit):
            raise ValueError(f"mastery_audit.provenance: {directory}: inconsistent or uncommitted source")
        checked = audit(root, root / path, directory, source_commit)
        result["cases"][key] = checked
        initial = checked["layouts"]["initial.json"]
        subset = [row for row in episodes if row["case"] == key]
        for row in subset:
            if abs(row["return"] - (initial[0] - row["score"][0]) / initial[0]) > 1e-10:
                raise ValueError(f"mastery_audit.reward: {directory}: training return differs from retained area")
        champion = min([initial, *[row["score"] for row in subset]])
        if (champion != metadata["champions"][key] or champion != checked["layouts"]["training-best.json"]
                or champion != checked["traces"]["training-best-trace.json"]["score"]):
            raise ValueError(f"mastery_audit.champion: {directory}: selected training layout or replay differs")
        start_path = directory / "training-best-start.json"
        if start_path.exists():
            selected = json.loads(start_path.read_text())
            episode = next(row for row in subset if row["episode"] == selected["episode"])
            if (selected != {"prefix": starts[key][str(episode["remaining"])], **episode}
                    or selected["score"] != champion):
                raise ValueError(f"mastery_audit.champion_start: {start_path}: start differs from selected training episode")
        elif champion != initial:
            raise ValueError(f"mastery_audit.champion_start: {directory}: missing selected start")
        evaluations = rows(directory / "evaluations.jsonl")
        if [(row["commands"], row["rollout_seed"]) for row in evaluations] != [
                (count, config["evaluation_seed_start"] + i) for count in config["checkpoints"] for i in range(config["evaluation_seeds"])]:
            raise ValueError(f"mastery_audit.evaluation_seeds: {directory}: frozen seed schedule differs")
        catalog = Catalog(root, root / path)
        for count in config["checkpoints"]:
            selected = [row for row in evaluations if row["commands"] == count]
            summary = next(row for row in summaries if row["case"] == key and row["commands"] == count)
            values = np.asarray([row["return"] for row in selected])
            expected = {"mean_return": float(values.mean()), "median_return": float(np.median(values)),
                        "max_return": float(values.max()), "improvement_frequency": float((values > 0).mean()),
                        "multiple_improvement_frequency": sum(row["counts"].get("positive_rewards", 0) >= 2 for row in selected) / len(selected)}
            if any(abs(summary[field] - value) > 1e-12 for field, value in expected.items()):
                raise ValueError(f"mastery_audit.summary: {directory}: {count}: aggregate differs from rows")
            for row in selected:
                if row["counts"]["commands"] != config["horizon"] or abs(row["return"] - (initial[0] - row["score"][0]) / initial[0]) > 1e-10:
                    raise ValueError(f"mastery_audit.evaluation_reward: {directory}: {count}: reward or horizon differs")
            ordered = sorted(selected, key=lambda row: row["score"])
            for label, row in (("best", ordered[0]), ("median", ordered[len(ordered)//2])):
                accepted = check(catalog, json.loads((directory / f"evaluation-{count}-{label}.json").read_text()))
                if not accepted["valid"] or accepted["score"] != row["score"]:
                    raise ValueError(f"mastery_audit.selected: {directory}: {count}: invalid or mismatched selected layout")
            if checked["traces"][f"evaluation-{count}-representative-trace.json"]["score"] != ordered[len(ordered)//2]["score"]:
                raise ValueError(f"mastery_audit.replay: {directory}: {count}: representative trace differs")
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
    torch.set_num_interop_threads(1)
    print(json.dumps(audit_study(args.root.resolve(), args.protocol.resolve(), args.output.resolve(), args.source_commit), indent=2))


if __name__ == "__main__":
    main()
