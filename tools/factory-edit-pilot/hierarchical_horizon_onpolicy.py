"""Test whether confirmed horizon-12 search transfers through on-policy training."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import statistics

from hierarchical_success_onpolicy import SOURCE, initial_models, train
from hierarchical_target_learner import (BINARY, GENERATION, SAMPLING, SEARCH,
                                         STREAMS, cli, identity, outcomes, panel)
from ppo_harness_state import read, sha, write


CONTROL_PROTOCOL = Path("docs/reports/2026-09-26-hierarchical-success-onpolicy-protocol.json")
CONTROL_PROGRESS = Path("docs/reports/2026-09-26-hierarchical-success-onpolicy-progress.json")
HORIZON = 12
SEED = 20260922


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-horizon-onpolicy: {message}")


def train_panel(root, stream, generation):
    return root / "panels" / f"train-{stream}-g{generation}.json"


def checkpoint(root, stream, generation):
    if generation < 0:
        return root / "inputs/outer-h12"
    return root / "training" / f"{stream}-g{generation}-h12" / "outer"


def search_config(root):
    return root / "configs/search-h12.json"


def semantic_search_sha256(root):
    # The search report hashes the parsed Rust config serialized compactly,
    # while the protocol records the source file's exact bytes separately.
    encoded = json.dumps(read(search_config(root)), separators=(",", ":"))
    return hashlib.sha256(encoded.encode()).hexdigest()


def control():
    protocol = read(CONTROL_PROTOCOL)
    progress = read(CONTROL_PROGRESS)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"] == 18
            and progress["protocol_sha256"] == sha(CONTROL_PROTOCOL)
            and progress["baseline"]["completed"] == 26,
            "retained horizon-six control is incomplete")
    return protocol, progress


def prepare(root, commit):
    require(not root.exists(), "output already exists")
    old_protocol, old_progress = control()
    lower, outer = initial_models()
    seen = set()
    for stream in STREAMS:
        source = read(SOURCE / "inputs/panels" / f"{stream}.json")
        require(len(source["cases"]) == 16, f"source panel differs: {stream}")
        for case in source["cases"]:
            key = identity(case)
            require(key not in seen, "duplicate source problem")
            seen.add(key)

    root.mkdir(parents=True)
    reference_sha = panel(root / "reference", "diagnostic", 20262550, 512, 64, seen)
    require(reference_sha == old_protocol["screen_diagnostic_sha256"],
            "screen diagnostic differs")
    panels = {}
    for index, stream in enumerate(STREAMS, 1):
        for generation in range(3):
            name = f"train-{stream}-g{generation}"
            panels[name] = panel(root, name, 20262700 + 100 * index + generation,
                                 512, 32, seen,
                                 namespace=f"hierarchical-recovery-onpolicy-{name}-v1")
            require(panels[name] == old_protocol["panels"][name],
                    f"training panel changed: {name}")
    panels["diagnostic"] = panel(root, "onpolicy-diagnostic", 20262950,
                                  1024, 64, seen,
                                  namespace="hierarchical-recovery-onpolicy-diagnostic-v1")
    require(panels["diagnostic"] == old_protocol["panels"]["diagnostic"]
            and len(seen) == 48 + 64 + 9 * 32 + 64,
            "diagnostic or disjointness changed")

    search = read(SEARCH)
    require(search["horizon"] == 6 and search["simulations_per_move"] == 128
            and search["root_selection"]["candidates"] == 16
            and search["root_selection"]["anchor_candidates"] == 8
            and search["root_selection"]["seed"] == SEED
            and "gumbel_assignment" not in search["root_selection"],
            "horizon-six source search changed")
    search["horizon"] = HORIZON
    config = search_config(root)
    config.parent.mkdir(parents=True)
    write(config, search)
    copied = checkpoint(root, STREAMS[0], -1)
    copied.mkdir(parents=True)
    for name in ("state.safetensors", "parameter-order.json"):
        shutil.copy2(outer / name, copied / name)
    outer_config = read(outer / "config.json")
    require(outer_config["search"] == read(SEARCH),
            "initial outer search contract changed")
    outer_config["search"] = search
    write(copied / "config.json", outer_config)

    protocol = {
        "schema": "aic-hierarchical-target-learner-protocol-v1",
        "phase": "confirmation", "contrast": "horizon12-vs-horizon6-current",
        "experiment_commit": commit, "planned_records": 9,
        "runner_sha256": sha(Path(__file__)),
        "helper_sha256": sha(Path(__file__).with_name("hierarchical_target_learner.py")),
        "training_helper_sha256": sha(Path(__file__).with_name(
            "hierarchical_success_onpolicy.py")),
        "tracking_sha256": sha(Path(__file__).with_name(
            "hierarchical_target_learner_tracking.py")),
        "control_protocol_sha256": sha(CONTROL_PROTOCOL),
        "control_progress_sha256": sha(CONTROL_PROGRESS),
        "binary_sha256": sha(BINARY), "source_search_sha256": sha(SEARCH),
        "search_sha256": sha(config), "generation_sha256": sha(GENERATION),
        "sampling_sha256": sha(SAMPLING),
        "lower_sha256": sha(lower / "state.safetensors"),
        "outer_sha256": sha(outer / "state.safetensors"),
        "outer_h12_config_sha256": sha(copied / "config.json"),
        "outer_h12_order_sha256": sha(copied / "parameter-order.json"),
        "screen_diagnostic_sha256": reference_sha,
        "source_panels": {stream: sha(SOURCE / "inputs/panels" / f"{stream}.json")
                          for stream in STREAMS},
        "panels": panels,
        "numeric_gate": {
            "final_mean_completion_gain": 4,
            "final_stream_wins": 2,
            "final_mean_fewer_lost_start_successes": 3,
            "final_positive_retention_streams": 2,
            "generation_2_3_mean_completion_gain": 0,
            "all_seed_mean_completion_gain": 4,
            "positive_seed_means": 2,
        },
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {
        "schema": "aic-hierarchical-horizon-onpolicy-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared", "baseline": old_progress["baseline"],
        "records": [],
    })
    print("prepared=3 streams x 3 horizon-12 generations; paired horizon-6 control retained")


def validate(root, require_observer=True):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    old_protocol, old_progress = control()
    lower, outer = initial_models()
    copied = checkpoint(root, STREAMS[0], -1)
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["helper_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_target_learner.py"))
            and protocol["training_helper_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_success_onpolicy.py"))
            and protocol["tracking_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_target_learner_tracking.py"))
            and protocol["control_protocol_sha256"] == sha(CONTROL_PROTOCOL)
            and protocol["control_progress_sha256"] == sha(CONTROL_PROGRESS)
            and progress["baseline"] == old_progress["baseline"]
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["source_search_sha256"] == sha(SEARCH)
            and protocol["search_sha256"] == sha(search_config(root))
            and protocol["generation_sha256"] == sha(GENERATION)
            and protocol["sampling_sha256"] == sha(SAMPLING)
            and protocol["lower_sha256"] == sha(lower / "state.safetensors")
            and protocol["outer_sha256"] == sha(outer / "state.safetensors")
            and protocol["outer_h12_config_sha256"] == sha(copied / "config.json")
            and protocol["outer_h12_order_sha256"] == sha(copied / "parameter-order.json")
            and protocol["screen_diagnostic_sha256"]
            == sha(root / "reference/panels/diagnostic.json"),
            "fixed input changed")
    require(old_protocol["panels"] == protocol["panels"],
            "paired panels changed")
    for stream in STREAMS:
        require(protocol["source_panels"][stream]
                == sha(SOURCE / "inputs/panels" / f"{stream}.json"),
                f"source panel changed: {stream}")
        for generation in range(3):
            name = f"train-{stream}-g{generation}"
            require(protocol["panels"][name] == sha(train_panel(root, stream, generation)),
                    f"training panel changed: {name}")
    require(protocol["panels"]["diagnostic"]
            == sha(root / "panels/onpolicy-diagnostic.json"),
            "diagnostic panel changed")
    if require_observer:
        observer = read(root / "wandb-observer.json")
        require(observer["protocol_sha256"] == sha(root / "protocol.json")
                and observer["status"] in ("ready", "running", "finished")
                and observer["remote_confirmed_step"] >= 0,
                "W&B readiness not remotely acknowledged")
    return protocol, progress, old_progress


def search(root, name, source_panel, lower, outer, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    seconds = None
    if not output.exists():
        seconds = cli(root, name, [
            "hierarchical-search-harness", "--config", search_config(root),
            "--panel", source_panel, "--lower-training-output", lower,
            "--outer-training-output", outer, "--catalog-root", "data/game/normalized",
            "--output", output, "--device", "cuda:0",
        ])
    report = read(output)
    require(report["status"] == "complete"
            and report["panel_sha256"] == sha(source_panel)
            and report["search_config_sha256"] == semantic_search_sha256(root)
            and report["lower_checkpoint_sha256"] == sha(lower / "state.safetensors")
            and report["outer_checkpoint_sha256"] == sha(outer / "state.safetensors"),
            f"search lineage differs: {name}")
    return report, seconds


def evaluate(root, name, outer):
    lower, _ = initial_models()
    source_panel = root / "panels/onpolicy-diagnostic.json"
    output = root / "evaluations" / f"{name}.json"
    report, seconds = search(root, f"evaluate-{name}", source_panel, lower, outer, output)
    cases = read(source_panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    return dict(outcomes(report, order, easy), report_sha256=sha(output),
                evaluation_seconds=seconds)


def run(root, max_records):
    protocol, progress, old_progress = validate(root)
    lower, _ = initial_models()
    if "initial_h12" not in progress:
        progress["initial_h12"] = evaluate(root, "start-h12", checkpoint(root, STREAMS[0], -1))
        write(root / "progress.json", progress)
    progress["status"] = "running"
    write(root / "progress.json", progress)
    added = 0
    for stream in STREAMS:
        for generation in range(3):
            name = f"{stream}-g{generation}-h12"
            retained = next((row for row in progress["records"] if row["name"] == name), None)
            if retained is not None:
                require(sha(checkpoint(root, stream, generation) / "state.safetensors")
                        == retained["outer_checkpoint_sha256"],
                        f"retained model changed: {name}")
                continue
            if max_records is not None and added >= max_records:
                return
            outer = checkpoint(root, stream, generation - 1)
            if generation:
                previous = next(row for row in progress["records"]
                                if row["name"] == f"{stream}-g{generation-1}-h12")
                require(sha(outer / "state.safetensors")
                        == previous["outer_checkpoint_sha256"],
                        f"previous model changed: {name}")
            source_panel = train_panel(root, stream, generation)
            collection = root / "collections" / f"{name}.json"
            source, search_seconds = search(root, f"collect-{name}", source_panel,
                                            lower, outer, collection)
            require(len(source["cases"]) == 32, "collection case count differs")
            collected = sum(case["mcts"]["fully_validated"] for case in source["cases"])
            output, training = train(root, name, source_panel, collection, outer)
            result = evaluate(root, name, output / "outer")
            comparison = next(row for row in old_progress["records"]
                              if row["name"] == f"{stream}-g{generation}-current")
            baseline = progress["baseline"]["case_success"]
            row = dict(result, name=name, stream=stream, arm="horizon12",
                       generation=generation + 1,
                       gained_from_start=sum(a == "0" and b == "1" for a, b in
                                             zip(baseline, result["case_success"])),
                       lost_from_start=sum(a == "1" and b == "0" for a, b in
                                           zip(baseline, result["case_success"])),
                       control_completed=comparison["completed"],
                       control_lost_from_start=comparison["lost_from_start"],
                       collection_completed=collected,
                       collection_sha256=sha(collection),
                       training_report_sha256=sha(output / "report.json"),
                       outer_checkpoint_sha256=training["outer_checkpoint_sha256"],
                       initial_outer_loss=training["initial_outer_loss"],
                       final_outer_loss=training["final_outer_loss"],
                       search_seconds=search_seconds,
                       training_seconds=training["elapsed_seconds"])
            progress["records"].append(row)
            write(root / "progress.json", progress)
            added += 1
            print(f"horizon12={name} completed={row['completed']}/64 "
                  f"control={comparison['completed']}/64 "
                  f"lost={row['lost_from_start']}", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "confirmation incomplete")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def summarize_primary(root):
    protocol, progress, old_progress = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "confirmation incomplete")
    by = {(row["stream"], row["generation"]): row for row in progress["records"]}
    controls = {(row["stream"], row["generation"]): row
                for row in old_progress["records"] if row["arm"] == "current"}
    completion = [by[(stream, 3)]["completed"] - controls[(stream, 3)]["completed"]
                  for stream in STREAMS]
    retention = [controls[(stream, 3)]["lost_from_start"]
                 - by[(stream, 3)]["lost_from_start"] for stream in STREAMS]
    later = [by[(stream, generation)]["completed"]
             - controls[(stream, generation)]["completed"]
             for stream in STREAMS for generation in (2, 3)]
    gate = protocol["numeric_gate"]
    checks = {
        "final_mean_completion_gain": statistics.mean(completion)
                                      >= gate["final_mean_completion_gain"],
        "final_stream_wins": sum(value > 0 for value in completion)
                             >= gate["final_stream_wins"],
        "final_mean_fewer_lost_start_successes": statistics.mean(retention)
                                                 >= gate["final_mean_fewer_lost_start_successes"],
        "final_positive_retention_streams": sum(value > 0 for value in retention)
                                            >= gate["final_positive_retention_streams"],
        "generation_2_3_mean_completion_gain": statistics.mean(later)
                                               >= gate["generation_2_3_mean_completion_gain"],
    }
    result = {
        "schema": "aic-hierarchical-horizon-onpolicy-primary-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "control_progress_sha256": sha(CONTROL_PROGRESS),
        "initial_h6_completed": progress["baseline"]["completed"],
        "initial_h12_completed": progress["initial_h12"]["completed"],
        "final_completion_differences": completion,
        "final_retention_differences": retention,
        "later_completion_differences": later,
        "primary_checks": checks,
        "passes_primary_gate": all(checks.values()),
        "stress_required_for_final_gate": all(checks.values()),
    }
    write(root / "primary-summary.json", result)
    print(f"passes_primary_gate={result['passes_primary_gate']}")


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
