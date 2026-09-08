"""Retain the first jointly mastered policy and evaluate untouched root seed banks."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

import torch

from evidence import provenance, write
from native import Runtime
from native_training_comparison import policy_hash, rows
from policy import Policy
from reference.acceptance import Catalog, check
from reference.environment import Environment, serialize
from two_case import evaluate_cases, validate_compatibility


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def first_mastered(gates, roots, required):
    """Selection depends exclusively on development controller state."""
    for gate in gates:
        state = gate["transition"]["after"]
        if all(state[key]["level"] == level and state[key]["streak"] >= required
               for key, level in roots.items()):
            return gate["commands"]
    return None


def select_source(source, protocol):
    metadata = read(source / "metadata.json")
    audit = read(source / "study-audit.json")
    starts = read(source / "starts.json")
    roots = metadata["root_levels"]
    if (metadata["status"] != "complete" or audit["status"] != "passed"
            or metadata.get("experiment_family") != "case_curriculum"
            or metadata["condition"] != protocol["condition"] or audit["condition"] != protocol["condition"]
            or metadata["seed"] != audit["seed"] or metadata["seed"] not in protocol["training_seeds"]
            or audit["metadata_sha256"] != digest(source / "metadata.json")
            or not audit["source_commit"].startswith(protocol["source_commit"])
            or set(roots) != set(metadata["protocol"]["cases"])
            or any(starts[key][str(level)] or level != max(map(int, starts[key])) for key, level in roots.items())):
        raise ValueError(f"selection.source: {source}: incomplete, unaudited or mismatched source")
    count = first_mastered(rows(source / "assessments.jsonl"), roots,
                           metadata["protocol"]["assessment"]["consecutive_passes"])
    if count is None:
        return metadata, None, None
    checkpoint = source / f"checkpoint-{count}.pt"
    if digest(checkpoint) != audit["checkpoints_sha256"][str(count)]:
        raise ValueError(f"selection.source: {checkpoint}: checkpoint hash differs from audit")
    saved = torch.load(checkpoint, map_location="cpu", weights_only=False)
    if (saved["metadata"]["completed_commands"] != count
            or saved["metadata"]["seed"] != metadata["seed"]
            or not all(saved["metadata"]["controller"][key]["level"] == level
                       and saved["metadata"]["controller"][key]["streak"] >= metadata["protocol"]["assessment"]["consecutive_passes"]
                       for key, level in roots.items())):
        raise ValueError(f"selection.source: {checkpoint}: selected checkpoint is not jointly mastered")
    return metadata, count, saved


def verify_case(root, case, config, directory, count):
    catalog = Catalog(root, root / case)
    episodes = rows(directory / "evaluations.jsonl")
    ordered = sorted(episodes, key=lambda row: row["score"])
    for label, episode in (("best", ordered[0]), ("median", ordered[len(ordered)//2])):
        accepted = check(catalog, read(directory / f"evaluation-{label}.json"))
        if not accepted["valid"] or accepted["score"] != episode["score"]:
            raise ValueError(f"selection.validation: {directory}: invalid {label} layout")
    env = Environment(catalog, config)
    trace = read(directory / f"evaluation-{count}-representative-trace.json")
    for row in trace:
        if row["command"] not in env.candidates().to_dicts():
            raise ValueError(f"selection.validation: {directory}: trace command outside domain")
        reward, _, outcome = env.step(row["command"])
        if abs(reward-row["reward"]) > 1e-10 or any(outcome[k] != row[k] for k in ("applied", "complete", "improved")):
            raise ValueError(f"selection.validation: {directory}: trace transition differs")
    accepted = check(catalog, serialize(env.best))
    if len(trace) != config["horizon"] or not accepted["valid"] or accepted["score"] != ordered[len(ordered)//2]["score"]:
        raise ValueError(f"selection.validation: {directory}: representative outcome differs")
    return {"status": "passed", "representative_score": accepted["score"], "commands": len(trace)}


def run(root, protocol_path, library, output):
    protocol = read(protocol_path)
    if (protocol.get("schema_version") != 1 or protocol.get("selection") != "first_joint_root_mastery"
            or len(set(protocol["training_seeds"])) != len(protocol["training_seeds"])
            or not protocol["training_seeds"] or protocol["evaluation_episodes"] < 2
            or not 1 <= protocol["required_successes"] <= protocol["evaluation_episodes"]
            or protocol["horizon"] <= 0 or len(protocol["seed_banks"]) != 2
            or abs(protocol["seed_banks"][0]-protocol["seed_banks"][1]) < protocol["evaluation_episodes"]):
        raise ValueError(f"selection.protocol: {protocol_path}: invalid selection or evaluation contract")
    if output.exists():
        raise ValueError(f"selection.protocol: {output}: output must be fresh")
    output.mkdir(parents=True)
    result = {"schema_version": 1, "protocol": protocol, "protocol_sha256": digest(protocol_path),
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "status": "running", "policies": []}
    stage, source = "selection", root / protocol["source_run_root"]
    try:
        # Resolve every policy before evaluating any outcome bank.
        selected = []
        start = time.perf_counter()
        for seed in protocol["training_seeds"]:
            source = root / protocol["source_run_root"] / f'{protocol["condition"]}-seed{seed}'
            metadata, count, saved = select_source(source, protocol)
            if metadata["seed"] != seed:
                raise ValueError(f"selection.source: {source}: wrong seed directory")
            if count is None:
                result.update(status="not_met", diagnostic={"code": "selection.no_mastered_policy", "severity": "info",
                              "stage": stage, "source": str(source), "message": "No jointly mastered checkpoint exists"})
                write(output / "selection.json", result)
                return result
            selected.append((source, metadata, count, saved))
        result["selection_seconds"] = time.perf_counter()-start
        stage = "evaluation"
        for source, metadata, count, saved in selected:
            config = {**metadata["config"], "device": "cpu", "horizon": protocol["horizon"],
                      "evaluation_seeds": protocol["evaluation_episodes"]}
            torch.set_num_threads(config["threads"])
            runtimes = {key: Runtime(root, root / path, config, library)
                        for key, path in metadata["protocol"]["cases"].items()}
            validate_compatibility(runtimes)
            runtime_sources = {key: provenance(runtime, protocol_path) for key, runtime in runtimes.items()}
            for key, current in runtime_sources.items():
                original = metadata["provenance"][key]
                required = [runtimes[key].path, *runtimes[key].paths.values(),
                            root / "tools/factory-edit-pilot/policy.py"]
                if not current["git_worktree_clean"] or any(
                        digest(Path(path)) != original["files_sha256"][str(Path(path).resolve().relative_to(root))]
                        for path in required):
                    raise ValueError(f"selection.source: {source}: changed policy/input or dirty evaluation source")
                if current["native"]["build"]["files_sha256"] != original["native"]["build"]["files_sha256"]:
                    raise ValueError(f"selection.source: {source}: changed native environment")
            example, _ = next(iter(runtimes.values())).new_engine().observe()
            actor = Policy(example, normalization=config.get("normalization", "none"))
            actor.load_state_dict(saved["evaluation_policy"])
            before = policy_hash(actor.state_dict())
            directory = output / f'seed{metadata["seed"]}'
            directory.mkdir()
            record = {"seed": metadata["seed"], "commands": count, "source": str(source),
                      "checkpoint_sha256": digest(source / f"checkpoint-{count}.pt"),
                      "source_audit_sha256": digest(source / "study-audit.json"), "policy_sha256": before,
                      "historical_training_seconds": metadata["training_seconds"], "provenance": runtime_sources, "banks": []}
            torch.save({"schema_version": 1, "kind": "inference_policy", "policy": actor.state_dict(),
                        "config": config, "dimensions": actor.dimensions, "source": dict(record)}, directory / "selected-policy.pt")
            record["export_sha256"] = digest(directory / "selected-policy.pt")
            for bank in protocol["seed_banks"]:
                config["evaluation_seed_start"] = bank
                bank_directory = directory / f"bank{bank}"
                for key in runtimes:
                    (bank_directory / key).mkdir(parents=True)
                summaries = evaluate_cases(actor, runtimes, config, bank_directory, count,
                                           saved["metadata"]["case_commands"], saved["metadata"]["training_seconds"])
                for key, summary in summaries.items():
                    episodes = rows(bank_directory / key / "evaluations.jsonl")
                    successes = sum(row["return"] > 0 for row in episodes)
                    validation = verify_case(root, metadata["protocol"]["cases"][key], config, bank_directory / key, count)
                    record["banks"].append({"seed_start": bank, "case": key, "successes": successes,
                                            "passed": successes >= protocol["required_successes"],
                                            "summary": summary, "validation": validation})
            if policy_hash(actor.state_dict()) != before:
                raise ValueError(f"selection.validation: {source}: evaluation mutated policy state")
            result["policies"].append(record)
            write(output / "selection.json", result)
        result["status"] = "passed" if all(cell["passed"] for p in result["policies"] for cell in p["banks"]) else "not_met"
        result["evaluation_seconds"] = time.perf_counter()-start-result["selection_seconds"]
    except BaseException as error:
        result.update(status="interrupted", diagnostic={"code": "selection.interrupted", "severity": "error",
                      "stage": stage, "source": str(source), "message": str(error)})
        write(output / "selection.json", result)
        raise
    write(output / "selection.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = run(args.root.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({"status": result["status"], "output": str(args.output)}))


if __name__ == "__main__":
    main()
