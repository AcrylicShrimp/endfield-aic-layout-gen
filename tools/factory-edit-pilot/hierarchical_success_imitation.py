"""Screen successful-action imitation on frozen hierarchical search collections."""

import argparse
from pathlib import Path
import statistics

from hierarchical_target_learner import (BINARY, GENERATION, SAMPLING, SEARCH,
                                         STREAMS, cli, identity, outcomes, panel, search)
from ppo_harness_state import read, sha, write


ARMS = ("current", "failed-off", "success-on", "success-off")
SOURCE = Path("artifacts/recovery-search-v2")
MASS = 0.8


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-success-imitation: {message}")


def source_collection(stream):
    return SOURCE / "raw" / stream / "base-20260922.json"


def source_panel(stream):
    return SOURCE / "inputs/panels" / f"{stream}.json"


def changed_target(source, output, manifest):
    require(source.is_file() and not output.exists() and not manifest.exists(),
            "source missing or transformed output already exists")
    report = read(source)
    require(report["schema"] in ("aic-hierarchical-search-report-v7", "aic-hierarchical-search-report-v8", "aic-hierarchical-search-report-v9")
            and report["status"] == "complete" and report["partition"] == "training"
            and report["root_records_retained"], "invalid source report")
    successful_roots = 0
    failed_roots = 0
    total_variation = 0.0
    for case in report["cases"]:
        records = case["mcts"]["root_records"]
        require(records, f"case has no roots: {case['id']}")
        for record in records:
            actions = record["actions"]
            selected = record["selected_action"]
            require(0 <= selected < len(actions), "invalid selected action")
            original = [action["policy_target"] for action in actions]
            require(all(isinstance(p, (int, float)) and p >= 0 for p in original)
                    and abs(sum(original) - 1.0) < 1e-5,
                    "source policy target is not normalized")
            require(record["later_fully_validated"]
                    == case["mcts"]["fully_validated"],
                    "root/episode success differs")
            if not record["later_fully_validated"]:
                failed_roots += 1
                continue
            for index, action in enumerate(actions):
                action["policy_target"] = ((1.0 - MASS) * original[index]
                                           + (MASS if index == selected else 0.0))
            total_variation += MASS * (1.0 - original[selected])
            successful_roots += 1
    require(successful_roots and failed_roots, "screen needs successes and failures")
    output.parent.mkdir(parents=True, exist_ok=True)
    manifest.parent.mkdir(parents=True, exist_ok=True)
    write(output, report)
    # Prove that the policy target fields are the only changed report fields.
    original = read(source)
    for old_case, new_case in zip(original["cases"], report["cases"]):
        for old_root, new_root in zip(old_case["mcts"]["root_records"],
                                      new_case["mcts"]["root_records"]):
            for old_action, new_action in zip(old_root["actions"], new_root["actions"]):
                old_action["policy_target"] = new_action["policy_target"]
    require(original == report, "non-target report field changed")
    write(manifest, {
        "schema": "aic-hierarchical-success-imitation-transform-v1",
        "source_sha256": sha(source), "output_sha256": sha(output),
        "mass_on_selected": MASS,
        "successful_roots": successful_roots, "failed_roots": failed_roots,
        "mean_root_total_variation": total_variation / successful_roots,
        "panel_sha256": report["panel_sha256"],
        "outer_checkpoint_sha256": report["outer_checkpoint_sha256"],
    })
    return sha(manifest)


def prepare(root, commit):
    require(not root.exists(), "output exists")
    require(read(SOURCE / "progress.json")["status"] == "complete"
            and (SOURCE / "summary.json").is_file(), "search screen not closed")
    baseline = read(GENERATION)
    require(baseline["epochs"] == 100 and baseline["train_failed_policy"]
            and not baseline["train_lower"] and baseline["outer_states_per_epoch"] == 256,
            "generation config differs")
    root.mkdir(parents=True)
    off = dict(baseline, train_failed_policy=False)
    (root / "configs").mkdir()
    write(root / "configs/generation-failed-off.json", off)
    seen = set()
    source_hashes = {}
    for stream in STREAMS:
        source_hashes[stream] = {
            "panel": sha(source_panel(stream)), "collection": sha(source_collection(stream))}
        cases = read(source_panel(stream))["cases"]
        require(len(cases) == 16, "source case count differs")
        for case in cases:
            key = identity(case)
            require(key not in seen, "duplicate source problem")
            seen.add(key)
    diagnostic_sha = panel(root, "diagnostic", 20262550, 512, 64, seen)
    protocol = {
        "schema": "aic-hierarchical-target-learner-protocol-v1",
        "phase": "screen", "contrast": "success-selected",
        "experiment_commit": commit, "planned_records": 12,
        "runner_sha256": sha(Path(__file__)),
        "helper_sha256": sha(Path(__file__).with_name("hierarchical_target_learner.py")),
        "tracking_sha256": sha(Path(__file__).with_name(
            "hierarchical_target_learner_tracking.py")),
        "binary_sha256": sha(BINARY), "search_sha256": sha(SEARCH),
        "generation_sha256": sha(GENERATION),
        "generation_failed_off_sha256": sha(root / "configs/generation-failed-off.json"),
        "sampling_sha256": sha(SAMPLING),
        "source_hashes": source_hashes,
        "lower_sha256": sha(SOURCE / "inputs/models/lower/state.safetensors"),
        "outer_sha256": sha(SOURCE / "inputs/models/outer-128/state.safetensors"),
        "diagnostic_sha256": diagnostic_sha,
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {
        "schema": "aic-hierarchical-success-imitation-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print(f"prepared_streams=3 arms=4 diagnostic=64 disjoint_problems={len(seen)}")


def validate(root):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["helper_sha256"]
            == sha(Path(__file__).with_name("hierarchical_target_learner.py"))
            and protocol["tracking_sha256"]
            == sha(Path(__file__).with_name("hierarchical_target_learner_tracking.py"))
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["search_sha256"] == sha(SEARCH)
            and protocol["generation_sha256"] == sha(GENERATION)
            and protocol["generation_failed_off_sha256"]
            == sha(root / "configs/generation-failed-off.json")
            and protocol["sampling_sha256"] == sha(SAMPLING)
            and protocol["diagnostic_sha256"] == sha(root / "panels/diagnostic.json"),
            "protocol input changed")
    require(protocol["lower_sha256"]
            == sha(SOURCE / "inputs/models/lower/state.safetensors")
            and protocol["outer_sha256"]
            == sha(SOURCE / "inputs/models/outer-128/state.safetensors"),
            "checkpoint changed")
    for stream in STREAMS:
        expected = protocol["source_hashes"][stream]
        require(expected["panel"] == sha(source_panel(stream))
                and expected["collection"] == sha(source_collection(stream)),
                f"source changed: {stream}")
    observer = read(root / "wandb-observer.json")
    require(observer["protocol_sha256"] == sha(root / "protocol.json")
            and observer["status"] in ("ready", "running", "finished")
            and observer["remote_confirmed_step"] >= 0,
            "W&B readiness not remotely acknowledged")
    return protocol, progress


def evaluate(root, name, lower, outer):
    source = root / "panels/diagnostic.json"
    output = root / "evaluations" / f"{name}.json"
    report, seconds = search(root, f"evaluate-{name}", source, lower, outer, output)
    cases = read(source)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    return dict(outcomes(report, order, easy), report_sha256=sha(output),
                evaluation_seconds=seconds)


def train(root, name, stream, arm, collection):
    output = root / "training" / name
    output.parent.mkdir(parents=True, exist_ok=True)
    config = (root / "configs/generation-failed-off.json"
              if arm in ("failed-off", "success-off") else GENERATION)
    lower = SOURCE / "inputs/models/lower"
    outer = SOURCE / "inputs/models/outer-128"
    if not output.exists():
        args = ["train-hierarchical-generation", "--config", config,
                "--panel", source_panel(stream), "--collection", collection,
                "--lower-training-output", lower, "--outer-training-output", outer,
                "--catalog-root", "data/game/normalized", "--output", output,
                "--device", "cuda:0"]
        if output.with_name(f".{output.name}.pending").exists():
            args.append("--resume")
        cli(root, f"train-{name}", args)
    result = read(output / "report.json")
    require(result["status"] == "complete" and result["epochs"] == 100
            and result["updates"] == 1600 and not result["train_lower"]
            and result["train_failed_policy"]
            == (arm in ("current", "success-on"))
            and result["collection_sha256"] == sha(collection)
            and result["source_outer_checkpoint_sha256"]
            == sha(outer / "state.safetensors")
            and result["lower_checkpoint_sha256"] == sha(lower / "state.safetensors"),
            f"training lineage differs: {name}")
    return output, result


def run(root, max_records):
    protocol, progress = validate(root)
    lower = SOURCE / "inputs/models/lower"
    outer = SOURCE / "inputs/models/outer-128"
    base = evaluate(root, "start", lower, outer)
    if "baseline" not in progress:
        progress["baseline"] = base
        write(root / "progress.json", progress)
    else:
        require(progress["baseline"]["case_success"] == base["case_success"],
                "baseline changed")
    progress["status"] = "running"
    write(root / "progress.json", progress)
    added = 0
    for stream in STREAMS:
        source = source_collection(stream)
        transformed = root / "collections" / f"{stream}-success.json"
        manifest = root / "manifests" / f"{stream}-success.json"
        if not transformed.exists() and not manifest.exists():
            changed_target(source, transformed, manifest)
        require(read(manifest)["source_sha256"] == sha(source)
                and read(manifest)["output_sha256"] == sha(transformed),
                f"transform changed: {stream}")
        for arm in ARMS:
            name = f"{stream}-{arm}"
            if any(row["name"] == name for row in progress["records"]):
                continue
            if max_records is not None and added >= max_records:
                return
            collection = transformed if arm.startswith("success") else source
            output, report = train(root, name, stream, arm, collection)
            result = evaluate(root, name, output / "lower", output / "outer")
            row = dict(result, name=name, stream=stream, arm=arm,
                       gained_from_start=sum(a == "0" and b == "1" for a, b in zip(
                           base["case_success"], result["case_success"])),
                       lost_from_start=sum(a == "1" and b == "0" for a, b in zip(
                           base["case_success"], result["case_success"])),
                       collection_sha256=sha(collection),
                       training_report_sha256=sha(output / "report.json"),
                       outer_checkpoint_sha256=report["outer_checkpoint_sha256"],
                       initial_outer_loss=report["initial_outer_loss"],
                       final_outer_loss=report["final_outer_loss"])
            progress["records"].append(row)
            write(root / "progress.json", progress)
            added += 1
            print(f"arm={name} completed={row['completed']}/64 "
                  f"lost={row['lost_from_start']}", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "screen incomplete")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def summarize(root):
    protocol, progress = validate(root)
    require(progress["status"] == "complete", "screen incomplete")
    rows = {(row["stream"], row["arm"]): row for row in progress["records"]}
    comparisons = {}
    for arm, control in (("failed-off", "current"),
                         ("success-on", "current"),
                         ("success-off", "failed-off"),
                         ("success-off", "current")):
        retention = [rows[(stream, control)]["lost_from_start"]
                     - rows[(stream, arm)]["lost_from_start"] for stream in STREAMS]
        completion = [rows[(stream, arm)]["completed"]
                      - rows[(stream, control)]["completed"] for stream in STREAMS]
        comparisons[f"{arm}-vs-{control}"] = {
            "retention_differences": retention,
            "completion_differences": completion,
            "mean_retention_difference": statistics.mean(retention),
            "mean_completion_difference": statistics.mean(completion),
            "passes_gate": (statistics.mean(retention) >= 3
                            and sum(value > 0 for value in retention) >= 2
                            and statistics.mean(completion) >= -2),
        }
    result = {"schema": "aic-hierarchical-success-imitation-summary-v1",
              "protocol_sha256": sha(root / "protocol.json"),
              "progress_sha256": sha(root / "progress.json"),
              "comparisons": comparisons}
    write(root / "summary.json", result)
    for name, row in comparisons.items():
        print(f"{name}: retention={row['retention_differences']} "
              f"completion={row['completion_differences']} gate={row['passes_gate']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    elif args.command == "run":
        run(args.output, args.max_records)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
