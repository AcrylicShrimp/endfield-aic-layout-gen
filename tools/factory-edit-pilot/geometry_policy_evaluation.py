"""Evaluate one retained policy on frozen development and held-out geometries."""

import argparse
import json
from pathlib import Path
import subprocess
import time

import torch

from evidence import provenance, write
from geometry_augmentation import digest as value_digest
from mastery_policy_selection import digest, read, select_source, verify_case
from native import Runtime
from native_training_comparison import policy_hash, rows
from policy import Policy
from two_case import evaluate_cases, validate_compatibility


def admitted_cases(root, protocol):
    directory = (root / protocol["geometry_directory"]).resolve()
    manifest = read(directory / "manifest.json")
    if manifest.get("status") != "passed" or value_digest(manifest["cases"]) != protocol["manifest_cases_sha256"]:
        raise ValueError(f"geometry_evaluation.manifest: {directory}: manifest differs from frozen split")
    cases, hashes, identifiers = {key: {} for key in ("train", "development", "heldout")}, set(), set()
    for row in manifest["cases"]:
        path = (directory / row["path"]).resolve()
        if (row["split"] not in cases or not path.is_relative_to(directory)
                or row["id"] in identifiers or row["geometry_sha256"] in hashes
                or digest(path) != row["case_sha256"]):
            raise ValueError(f"geometry_evaluation.case: {path}: changed, duplicate or invalid split input")
        identifiers.add(row["id"])
        hashes.add(row["geometry_sha256"])
        cases[row["split"]][row["id"]] = path
    if any(not value for value in cases.values()):
        raise ValueError(f"geometry_evaluation.split: {directory}: empty split")
    return cases


def validate_protocol(protocol):
    if protocol.get("schema_version") != 1 or protocol.get("selection") != "first_joint_root_mastery":
        raise ValueError("geometry_evaluation.protocol: unsupported protocol or selection")
    banks = []
    for stage in ("development", "heldout"):
        spec = protocol[stage]
        if (type(spec["horizon"]) is not int or spec["horizon"] <= 0
                or type(spec["episodes"]) is not int or spec["episodes"] < 2
                or type(spec["required_successes"]) is not int
                or not 1 <= spec["required_successes"] <= spec["episodes"]
                or len(spec["seed_banks"]) != (1 if stage == "development" else 2)
                or any(type(seed) is not int or seed < 0 for seed in spec["seed_banks"])):
            raise ValueError(f"geometry_evaluation.protocol: invalid {stage} budget")
        banks.extend((seed, spec["episodes"]) for seed in spec["seed_banks"])
    if any(max(a, b) < min(a+n, b+m) for i, (a, n) in enumerate(banks) for b, m in banks[i+1:]):
        raise ValueError("geometry_evaluation.protocol: overlapping evaluation seed banks")
    return banks


def run(root, protocol_path, library, output):
    protocol = read(protocol_path)
    banks = validate_protocol(protocol)
    cases = admitted_cases(root, protocol)
    if output.exists():
        raise ValueError(f"geometry_evaluation.output: {output}: output must be fresh")
    output.mkdir(parents=True)
    started = time.perf_counter()
    result = {"schema_version": 1, "status": "running", "protocol": protocol,
              "protocol_sha256": digest(protocol_path), "stages": {},
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()}
    stage = "selection"
    try:
        source = root / protocol["source_run"]
        metadata, count, saved = select_source(source, protocol)
        if set((root / p).resolve() for p in metadata["protocol"]["cases"].values()) != set(cases["train"].values()):
            raise ValueError("geometry_evaluation.source: source training cases differ from declared training split")
        for key, path in metadata["protocol"]["cases"].items():
            expected = metadata["provenance"][key]["files_sha256"][str((root / path).resolve().relative_to(root))]
            if digest(root / path) != expected:
                raise ValueError(f"geometry_evaluation.source: changed training case: {path}")
        if count is None:
            result.update(status="not_met", diagnostic={"code": "geometry_evaluation.no_mastered_policy",
                          "severity": "info", "stage": stage, "message": "No jointly mastered training-root checkpoint"})
            write(output / "evaluation.json", result)
            return result
        source_banks = [(metadata["config"]["evaluation_seed_start"], metadata["config"]["evaluation_seeds"]),
                        (metadata["protocol"]["assessment"]["seed_start"], metadata["protocol"]["assessment"]["episodes"])]
        if any(max(a, b) < min(a+n, b+m) for a, n in banks for b, m in source_banks):
            raise ValueError("geometry_evaluation.protocol: outcome seeds overlap training assessments")
        config = {**metadata["config"], "device": "cpu"}
        torch.set_num_threads(config["threads"])
        first = next(iter(cases["train"].values()))
        runtime = Runtime(root, first, config, library)
        current = provenance(runtime, protocol_path)
        original = next(iter(metadata["provenance"].values()))
        required = [root / "tools/factory-edit-pilot/policy.py", *runtime.paths.values()]
        if (not current["git_worktree_clean"] or any(digest(path) != original["files_sha256"][str(path.relative_to(root))]
                                                    for path in required)
                or current["native"]["build"]["files_sha256"] != original["native"]["build"]["files_sha256"]):
            raise ValueError("geometry_evaluation.source: changed policy/catalog/native source or dirty worktree")
        example, _ = runtime.new_engine().observe()
        actor = Policy(example, normalization=config.get("normalization", "none"),
                       width_multiplier=config.get("width_multiplier", 1))
        actor.load_state_dict(saved["evaluation_policy"])
        actor.eval()
        before = policy_hash(actor.state_dict())
        result["selection"] = {"seed": metadata["seed"], "commands": count, "policy_sha256": before,
                               "checkpoint_sha256": digest(source / f"checkpoint-{count}.pt"),
                               "source_audit_sha256": digest(source / "study-audit.json"),
                               "historical_training_seconds": metadata["training_seconds"], "provenance": current}
        torch.save({"schema_version": 1, "kind": "inference_policy", "policy": actor.state_dict(),
                    "config": config, "dimensions": actor.dimensions, "source": result["selection"]},
                   output / "selected-policy.pt")
        result["selection"]["export_sha256"] = digest(output / "selected-policy.pt")
        for stage in ("development", "heldout"):
            spec = protocol[stage]
            result["stages"][stage] = []
            for bank in spec["seed_banks"]:
                eval_config = {**config, "horizon": spec["horizon"], "evaluation_seeds": spec["episodes"],
                               "evaluation_seed_start": bank}
                runtimes = {key: Runtime(root, path, eval_config, library) for key, path in cases[stage].items()}
                validate_compatibility({"training": runtime, **runtimes})
                destination = output / stage / f"bank{bank}"
                for key in runtimes:
                    (destination / key).mkdir(parents=True)
                summaries = evaluate_cases(actor, runtimes, eval_config, destination, count,
                                           {key: 0 for key in runtimes}, metadata["training_seconds"])
                for key, summary in summaries.items():
                    episodes = rows(destination / key / "evaluations.jsonl")
                    successes = sum(row["return"] > 0 and bool(row.get("translated_facilities")) for row in episodes)
                    validation = verify_case(root, cases[stage][key], eval_config, destination / key, count)
                    result["stages"][stage].append({"case": key, "seed_start": bank,
                        "translated_successes": successes, "passed": successes >= spec["required_successes"],
                        "summary": summary, "validation": validation})
                if policy_hash(actor.state_dict()) != before:
                    raise ValueError(f"geometry_evaluation.mutation: {stage}: evaluation changed policy or BN buffers")
                write(output / "evaluation.json", result)
            if not all(row["passed"] for row in result["stages"][stage]):
                result.update(status="not_met", diagnostic={"code": f"geometry_evaluation.{stage}_failed",
                              "severity": "info", "stage": stage, "message": f"{stage} translation criterion not met"})
                break
        else:
            result["status"] = "passed"
        result["total_seconds"] = time.perf_counter() - started
    except BaseException as error:
        result.update(status="interrupted", diagnostic={"code": "geometry_evaluation.interrupted", "stage": stage,
                      "severity": "error", "message": str(error)})
        write(output / "evaluation.json", result)
        raise
    write(output / "evaluation.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    result = run(Path(__file__).resolve().parents[2], args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({"status": result["status"], "output": str(args.output)}))


if __name__ == "__main__":
    main()
