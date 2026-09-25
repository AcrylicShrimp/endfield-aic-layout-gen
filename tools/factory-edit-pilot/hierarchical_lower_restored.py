"""Confirm H12 search with both hierarchical models trained on fresh panels."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import statistics

from hierarchical_horizon_onpolicy import HORIZON, control, require
from hierarchical_success_onpolicy import SOURCE, initial_models
from hierarchical_target_learner import (BINARY, SAMPLING, SEARCH, STREAMS, cli,
                                         identity, outcomes, panel)
from ppo_harness_state import read, sha, write


H12_ROOT = Path("artifacts/recovery-horizon-onpolicy-v2")
H12_PROTOCOL = Path("docs/reports/2026-09-26-hierarchical-h12-onpolicy-protocol.json")
MIXED_SUMMARY = Path("docs/reports/2026-09-26-hierarchical-h12-mixed-seed-summary.json")
GENERATION_LOWER = Path(
    "docs/benchmarks/hierarchical-update-budget/generation-100-lower.json")
TRACKING = Path(__file__).with_name("hierarchical_target_learner_tracking.py")
SEED = 20260922
ARMS = ("h6", "mixed", "h12")


def search_config(root, arm):
    return SEARCH if arm == "h6" else root / "configs/search-h12.json"


def semantic_search_sha256(path):
    return hashlib.sha256(json.dumps(read(path), separators=(",", ":")).encode()).hexdigest()


def train_panel(root, stream, generation):
    return root / "panels" / f"train-{stream}-g{generation}.json"


def checkpoint(root, stream, generation, arm):
    lower, outer = initial_models()
    if generation < 0:
        return lower, outer if arm == "h6" else root / "inputs/outer-h12"
    output = root / "training" / f"{stream}-g{generation}-{arm}"
    return output / "lower", output / "outer"


def prior_identities():
    protocol = read(H12_PROTOCOL)
    seen = set()
    paths = ([SOURCE / "inputs/panels" / f"{stream}.json" for stream in STREAMS]
             + [H12_ROOT / "reference/panels/diagnostic.json"]
             + [H12_ROOT / "panels" / f"train-{stream}-g{generation}.json"
                for stream in STREAMS for generation in range(3)]
             + [H12_ROOT / "panels/onpolicy-diagnostic.json"])
    for path in paths:
        for case in read(path)["cases"]:
            key = identity(case)
            require(key not in seen, f"prior panel overlap: {path}")
            seen.add(key)
    require(len(seen) == 48 + 64 + 9 * 32 + 64
            and sha(H12_ROOT / "panels/onpolicy-diagnostic.json")
            == protocol["panels"]["diagnostic"],
            "prior diagnostic changed")
    return seen, {str(path): sha(path) for path in paths}


def reserve_heldout(root, seen):
    name = "lower-restored-heldout"
    draft = root / "candidate-panels" / f"{name}.json"
    draft.parent.mkdir(parents=True, exist_ok=True)
    cli(root, f"prepare-{name}", [
        "prepare-hierarchical-panel", "--sampling", SAMPLING,
        "--catalog-root", "data/game/normalized", "--output", draft,
        "--partition", "held-out", "--cases", 2048,
        "--seed", 20264250, "--namespace", "hierarchical-lower-restored-heldout-v1",
        "--min-facilities", 2, "--max-facilities", 2,
        "--max-terminals", 6, "--max-connections", 6,
        "--max-turns", 10000, "--max-extension", 32,
    ])
    source = read(draft)
    require(source["partition"] == "held-out", "held-out partition changed")
    selected = []
    for case in source["cases"]:
        key = identity(case)
        if key not in seen:
            selected.append(case)
            seen.add(key)
        if len(selected) == 128:
            break
    require(len(selected) == 128, "too few disjoint held-out cases")
    source["cases"] = selected
    output = root / "panels/heldout.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    write(output, source)
    return sha(output)


def prepare(root, commit):
    require(not root.exists(), "output already exists")
    old_protocol, old_progress = control()
    h12_protocol = read(H12_PROTOCOL)
    mixed = read(MIXED_SUMMARY)
    require(not mixed["passes_mixed_selection_gate"]
            and old_progress["status"] == "complete"
            and h12_protocol["outer_sha256"] == old_protocol["outer_sha256"],
            "candidate selection or source model changed")
    generation = read(GENERATION_LOWER)
    require(generation["train_lower"] and generation["epochs"] == 100
            and generation["outer_states_per_epoch"] == 256
            and generation["batch_states"] == 16
            and generation["train_failed_policy"],
            "lower-restored training contract changed")
    original = read(SEARCH)
    deeper = read(H12_ROOT / "configs/search-h12.json")
    require(original["horizon"] == 6 and deeper["horizon"] == HORIZON
            and original["root_selection"]["seed"]
            == deeper["root_selection"]["seed"] == SEED
            and sha(H12_ROOT / "configs/search-h12.json")
            == h12_protocol["search_sha256"],
            "search contract changed")
    lower, outer = initial_models()
    prior_seen, prior_hashes = prior_identities()
    root.mkdir(parents=True)
    config = root / "configs/search-h12.json"
    config.parent.mkdir(parents=True)
    write(config, deeper)
    h12_outer = root / "inputs/outer-h12"
    h12_outer.mkdir(parents=True)
    for filename in ("state.safetensors", "parameter-order.json"):
        shutil.copy2(outer / filename, h12_outer / filename)
    outer_config = read(outer / "config.json")
    require(outer_config["search"] == original,
            "initial outer search contract changed")
    outer_config["search"] = deeper
    write(h12_outer / "config.json", outer_config)
    panels = {}
    for index, stream in enumerate(STREAMS, 1):
        for generation_index in range(3):
            name = f"train-{stream}-g{generation_index}"
            panels[name] = panel(root, name,
                                 20263500 + 100 * index + generation_index,
                                 512, 32, prior_seen,
                                 namespace=f"hierarchical-lower-restored-{name}-v1")
    panels["diagnostic"] = panel(
        root, "lower-diagnostic", 20264150, 1024, 64, prior_seen,
        namespace="hierarchical-lower-restored-diagnostic-v1")
    panels["heldout"] = reserve_heldout(root, prior_seen)
    require(len(prior_seen) == 48 + 64 + 9 * 32 + 64 + 9 * 32 + 64 + 128,
            "new panels overlap")
    write(root / "protocol.json", {
        "schema": "aic-hierarchical-target-learner-protocol-v1",
        "phase": "confirmation", "contrast": "lower-restored-h12-search",
        "experiment_commit": commit, "planned_records": 27,
        "runner_sha256": sha(Path(__file__)),
        "tracking_sha256": sha(TRACKING),
        "helper_sha256": sha(Path(__file__).with_name(
            "hierarchical_target_learner.py")),
        "binary_sha256": sha(BINARY),
        "sampling_sha256": sha(SAMPLING),
        "generation_sha256": sha(GENERATION_LOWER),
        "search_h6_sha256": sha(SEARCH),
        "search_h12_sha256": sha(config),
        "prior_h12_protocol_sha256": sha(H12_PROTOCOL),
        "mixed_summary_sha256": sha(MIXED_SUMMARY),
        "prior_panels": prior_hashes,
        "lower_sha256": sha(lower / "state.safetensors"),
        "lower_order_sha256": sha(lower / "parameter-order.json"),
        "outer_sha256": sha(outer / "state.safetensors"),
        "outer_order_sha256": sha(outer / "parameter-order.json"),
        "outer_h12_config_sha256": sha(h12_outer / "config.json"),
        "panels": panels,
        "selected_candidate": "h12",
        "numeric_gate": {
            "final_mean_completion_gain": 4,
            "final_stream_wins": 2,
            "final_mean_fewer_lost_start_successes": 3,
            "final_positive_retention_streams": 2,
            "generation_2_3_mean_completion_gain": 0,
        },
    })
    write(root / "progress.json", {
        "schema": "aic-hierarchical-lower-restored-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 streams x 3 generations x 3 diagnostic arms; "
          "lower training enabled; held-out reserved")


def validate(root, require_observer=True):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    lower, outer = initial_models()
    h12_outer = root / "inputs/outer-h12"
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["tracking_sha256"] == sha(TRACKING)
            and protocol["helper_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_target_learner.py"))
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["sampling_sha256"] == sha(SAMPLING)
            and protocol["generation_sha256"] == sha(GENERATION_LOWER)
            and protocol["search_h6_sha256"] == sha(SEARCH)
            and protocol["search_h12_sha256"] == sha(search_config(root, "h12"))
            and protocol["prior_h12_protocol_sha256"] == sha(H12_PROTOCOL)
            and protocol["mixed_summary_sha256"] == sha(MIXED_SUMMARY)
            and protocol["lower_sha256"] == sha(lower / "state.safetensors")
            and protocol["lower_order_sha256"] == sha(lower / "parameter-order.json")
            and protocol["outer_sha256"] == sha(outer / "state.safetensors")
            and protocol["outer_order_sha256"] == sha(outer / "parameter-order.json")
            and protocol["outer_h12_config_sha256"] == sha(h12_outer / "config.json")
            and sha(h12_outer / "state.safetensors") == protocol["outer_sha256"]
            and sha(h12_outer / "parameter-order.json")
            == protocol["outer_order_sha256"]
            and read(h12_outer / "config.json")["search"]
            == read(search_config(root, "h12"))
            and protocol["planned_records"] == 27
            and protocol["selected_candidate"] == "h12",
            "fixed input changed")
    for path, digest in protocol["prior_panels"].items():
        require(sha(Path(path)) == digest, f"prior panel changed: {path}")
    for stream in STREAMS:
        for generation_index in range(3):
            name = f"train-{stream}-g{generation_index}"
            require(protocol["panels"][name]
                    == sha(train_panel(root, stream, generation_index)),
                    f"training panel changed: {name}")
    require(protocol["panels"]["diagnostic"]
            == sha(root / "panels/lower-diagnostic.json")
            and protocol["panels"]["heldout"]
            == sha(root / "panels/heldout.json")
            and read(root / "panels/heldout.json")["partition"] == "held-out",
            "evaluation panel changed")
    if require_observer:
        observer = read(root / "wandb-observer.json")
        require(observer["protocol_sha256"] == sha(root / "protocol.json")
                and observer["status"] in ("ready", "running", "finished")
                and observer["remote_confirmed_step"] >= 0,
                "W&B readiness not remotely acknowledged")
    return protocol, progress


def search(root, name, source_panel, lower, outer, config, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    seconds = None
    if not output.exists():
        seconds = cli(root, name, [
            "hierarchical-search-harness", "--config", config,
            "--panel", source_panel, "--lower-training-output", lower,
            "--outer-training-output", outer,
            "--catalog-root", "data/game/normalized",
            "--output", output, "--device", "cuda:0"])
    report = read(output)
    require(report["status"] == "complete"
            and report["panel_sha256"] == sha(source_panel)
            and report["search_config_sha256"] == semantic_search_sha256(config)
            and report["lower_checkpoint_sha256"] == sha(lower / "state.safetensors")
            and report["outer_checkpoint_sha256"] == sha(outer / "state.safetensors"),
            f"search lineage differs: {name}")
    return report, seconds


def evaluate(root, name, lower, outer, config):
    source_panel = root / "panels/lower-diagnostic.json"
    output = root / "evaluations" / f"{name}.json"
    report, seconds = search(root, f"evaluate-{name}", source_panel,
                             lower, outer, config, output)
    cases = read(source_panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    return dict(outcomes(report, order, easy), report_sha256=sha(output),
                evaluation_seconds=seconds)


def train(root, name, source_panel, collection, lower, outer):
    output = root / "training" / name
    output.parent.mkdir(parents=True, exist_ok=True)
    if not output.exists():
        arguments = [
            "train-hierarchical-generation", "--config", GENERATION_LOWER,
            "--panel", source_panel, "--collection", collection,
            "--lower-training-output", lower, "--outer-training-output", outer,
            "--catalog-root", "data/game/normalized", "--output", output,
            "--device", "cuda:0"]
        if output.with_name(f".{output.name}.pending").exists():
            arguments.append("--resume")
        cli(root, f"train-{name}", arguments)
    report = read(output / "report.json")
    require(report["status"] == "complete" and report["train_lower"]
            and report["train_failed_policy"] and report["epochs"] == 100
            and report["updates"] >= 1600
            and report["panel_sha256"] == sha(source_panel)
            and report["collection_sha256"] == sha(collection)
            and report["source_lower_checkpoint_sha256"]
            == sha(lower / "state.safetensors")
            and report["source_outer_checkpoint_sha256"]
            == sha(outer / "state.safetensors")
            and report["lower_checkpoint_sha256"]
            == sha(output / "lower/state.safetensors")
            and report["outer_checkpoint_sha256"]
            == sha(output / "outer/state.safetensors"),
            f"training lineage differs: {name}")
    return output, report


def mixed_outer(root, stream, generation_index):
    _, source = checkpoint(root, stream, generation_index, "h6")
    output = root / "mixed-outer" / f"{stream}-g{generation_index}"
    if not output.exists():
        output.mkdir(parents=True)
        for filename in ("state.safetensors", "parameter-order.json"):
            shutil.copy2(source / filename, output / filename)
        config = read(source / "config.json")
        require(config["search"] == read(SEARCH),
                "h6 trained search contract changed")
        config["search"] = read(search_config(root, "h12"))
        write(output / "config.json", config)
    require(sha(output / "state.safetensors")
            == sha(source / "state.safetensors")
            and sha(output / "parameter-order.json")
            == sha(source / "parameter-order.json")
            and read(output / "config.json")["search"]
            == read(search_config(root, "h12")),
            "mixed checkpoint changed")
    return output


def run(root, max_records):
    protocol, progress = validate(root)
    if "baseline" not in progress:
        lower, outer = checkpoint(root, STREAMS[0], -1, "h6")
        progress["baseline"] = evaluate(root, "start-h6", lower, outer,
                                        search_config(root, "h6"))
        write(root / "progress.json", progress)
    if "initial_h12" not in progress:
        lower, outer = checkpoint(root, STREAMS[0], -1, "h12")
        progress["initial_h12"] = evaluate(root, "start-h12", lower, outer,
                                           search_config(root, "h12"))
        write(root / "progress.json", progress)
    baseline = progress["baseline"]["case_success"]
    require(len(baseline) == 64, "starting diagnostic mask differs")
    progress["status"] = "running"
    write(root / "progress.json", progress)
    added = 0
    for stream in STREAMS:
        for generation_index in range(3):
            for arm in ARMS:
                name = f"{stream}-g{generation_index}-{arm}"
                retained = next((row for row in progress["records"]
                                 if row["name"] == name), None)
                if retained is not None:
                    underlying = "h6" if arm == "mixed" else arm
                    lower, outer = checkpoint(root, stream, generation_index,
                                              underlying)
                    require(sha(lower / "state.safetensors")
                            == retained["lower_checkpoint_sha256"]
                            and sha(outer / "state.safetensors")
                            == retained["outer_checkpoint_sha256"],
                            f"retained model changed: {name}")
                    continue
                if max_records is not None and added >= max_records:
                    return
                config = search_config(root, arm)
                if arm == "mixed":
                    source = next(row for row in progress["records"]
                                  if row["name"] == f"{stream}-g{generation_index}-h6")
                    lower, outer = checkpoint(root, stream, generation_index, "h6")
                    shadow = mixed_outer(root, stream, generation_index)
                    result = evaluate(root, name, lower, shadow, config)
                    training = None
                    collection_sha = source["collection_sha256"]
                    collected = source["collection_completed"]
                    search_seconds = None
                else:
                    prior_lower, prior_outer = checkpoint(
                        root, stream, generation_index - 1, arm)
                    source_panel = train_panel(root, stream, generation_index)
                    collection = root / "collections" / f"{name}.json"
                    source_report, search_seconds = search(
                        root, f"collect-{name}", source_panel, prior_lower,
                        prior_outer, config, collection)
                    require(len(source_report["cases"]) == 32,
                            f"collection case count differs: {name}")
                    collected = sum(bool(case["mcts"]["fully_validated"])
                                    for case in source_report["cases"])
                    output, training = train(root, name, source_panel,
                                             collection, prior_lower, prior_outer)
                    lower, outer = output / "lower", output / "outer"
                    result = evaluate(root, name, lower, outer, config)
                    collection_sha = sha(collection)
                row = dict(result, name=name, stream=stream, arm=arm,
                           generation=generation_index + 1,
                           gained_from_start=sum(a == "0" and b == "1" for a, b in
                                                 zip(baseline, result["case_success"])),
                           lost_from_start=sum(a == "1" and b == "0" for a, b in
                                               zip(baseline, result["case_success"])),
                           collection_completed=collected,
                           collection_sha256=collection_sha,
                           lower_checkpoint_sha256=sha(lower / "state.safetensors"),
                           outer_checkpoint_sha256=sha(outer / "state.safetensors"),
                           training_report_sha256=(sha(output / "report.json")
                                                   if training else source[
                                                       "training_report_sha256"]),
                           initial_outer_loss=(training["initial_outer_loss"]
                                               if training else source[
                                                   "initial_outer_loss"]),
                           final_outer_loss=(training["final_outer_loss"]
                                             if training else source[
                                                 "final_outer_loss"]),
                           initial_lower_loss=(training["initial_lower_loss"]
                                               if training else source[
                                                   "initial_lower_loss"]),
                           final_lower_loss=(training["final_lower_loss"]
                                             if training else source[
                                                 "final_lower_loss"]),
                           search_seconds=search_seconds,
                           training_seconds=(training["elapsed_seconds"]
                                             if training else None))
                progress["records"].append(row)
                write(root / "progress.json", progress)
                added += 1
                print(f"lower-restored={name} completed={row['completed']}/64 "
                      f"lost={row['lost_from_start']}", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "integrated confirmation incomplete")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def summarize_primary(root):
    protocol, progress = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "integrated confirmation incomplete")
    by = {(row["stream"], row["generation"], row["arm"]): row
          for row in progress["records"]}
    candidate = protocol["selected_candidate"]
    completion = [by[(stream, 3, candidate)]["completed"]
                  - by[(stream, 3, "h6")]["completed"] for stream in STREAMS]
    retention = [by[(stream, 3, "h6")]["lost_from_start"]
                 - by[(stream, 3, candidate)]["lost_from_start"]
                 for stream in STREAMS]
    later = [by[(stream, generation_index, candidate)]["completed"]
             - by[(stream, generation_index, "h6")]["completed"]
             for stream in STREAMS for generation_index in (2, 3)]
    gate = protocol["numeric_gate"]
    checks = {
        "final_mean_completion_gain": statistics.mean(completion)
                                      >= gate["final_mean_completion_gain"],
        "final_stream_wins": sum(value > 0 for value in completion)
                             >= gate["final_stream_wins"],
        "final_mean_fewer_lost_start_successes": statistics.mean(retention)
                                                 >= gate[
                                                     "final_mean_fewer_lost_start_successes"],
        "final_positive_retention_streams": sum(value > 0 for value in retention)
                                            >= gate["final_positive_retention_streams"],
        "generation_2_3_mean_completion_gain": statistics.mean(later)
                                               >= gate["generation_2_3_mean_completion_gain"],
    }
    result = {
        "schema": "aic-hierarchical-lower-restored-primary-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "selected_candidate": candidate,
        "initial_h6_completed": progress["baseline"]["completed"],
        "initial_h12_completed": progress["initial_h12"]["completed"],
        "final_completion_differences": completion,
        "final_retention_differences": retention,
        "later_completion_differences": later,
        "primary_checks": checks,
        "passes_primary_gate": all(checks.values()),
        "heldout_required_for_final_gate": all(checks.values()),
    }
    write(root / "primary-summary.json", result)
    print(f"selected_candidate={candidate} passes_primary_gate={all(checks.values())}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize-primary"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        require(args.experiment_commit, "prepare needs --experiment-commit")
        prepare(args.output, args.experiment_commit)
    elif args.command == "run":
        run(args.output, args.max_records)
    else:
        summarize_primary(args.output)
