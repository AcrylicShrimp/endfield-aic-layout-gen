"""Confirm successful-action imitation over paired fresh on-policy generations."""

import argparse
from pathlib import Path
import statistics

from hierarchical_success_imitation import changed_target
from hierarchical_target_learner import (BINARY, GENERATION, SAMPLING, SEARCH,
                                         STREAMS, cli, identity, outcomes, panel, search)
from ppo_harness_state import read, sha, write


ARMS = ("current", "success-on")
SOURCE = Path("artifacts/recovery-search-v2")
SCREEN_PROTOCOL = Path("docs/reports/2026-09-25-hierarchical-success-imitation-protocol.json")
SCREEN_SUMMARY = Path("docs/reports/2026-09-25-hierarchical-success-imitation-summary.json")
SEEDS = (20260922, 20260923, 20260924)


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-success-onpolicy: {message}")


def initial_models():
    return SOURCE / "inputs/models/lower", SOURCE / "inputs/models/outer-128"


def panel_path(root, stream, generation):
    return root / "panels" / f"train-{stream}-g{generation}.json"


def checkpoint(root, stream, generation, arm):
    if generation < 0:
        return initial_models()[1]
    return root / "training" / f"{stream}-g{generation}-{arm}" / "outer"


def prepare(root, commit):
    require(not root.exists(), "output already exists")
    screen = read(SCREEN_SUMMARY)
    require(screen["comparisons"]["success-on-vs-current"]["passes_gate"]
            and not any(value["passes_gate"] for key, value in
                        screen["comparisons"].items() if key != "success-on-vs-current"),
            "screen selection changed")
    screen_protocol = read(SCREEN_PROTOCOL)
    lower, outer = initial_models()
    seen = set()
    for stream in STREAMS:
        source = read(SOURCE / "inputs/panels" / f"{stream}.json")
        require(len(source["cases"]) == 16, "source panel changed")
        for case in source["cases"]:
            key = identity(case)
            require(key not in seen, "duplicate screen source graph")
            seen.add(key)
    root.mkdir(parents=True)
    reference_sha = panel(root / "reference", "diagnostic", 20262550, 512, 64, seen)
    require(reference_sha == screen_protocol["diagnostic_sha256"],
            "frozen-screen diagnostic was not reconstructed")
    panels = {}
    for index, stream in enumerate(STREAMS, 1):
        for generation in range(3):
            name = f"train-{stream}-g{generation}"
            panels[name] = panel(root, name, 20262700 + 100 * index + generation,
                                 512, 32, seen,
                                 namespace=f"hierarchical-recovery-onpolicy-{name}-v1")
    panels["diagnostic"] = panel(root, "onpolicy-diagnostic", 20262950,
                                  1024, 64, seen,
                                  namespace="hierarchical-recovery-onpolicy-diagnostic-v1")
    require(len(seen) == 48 + 64 + 9 * 32 + 64, "panel graph overlap")
    search_configs = {}
    original = read(SEARCH)
    require(original["root_selection"]["seed"] == SEEDS[0]
            and original["horizon"] == 6
            and original["simulations_per_move"] == 128,
            "base search configuration changed")
    for seed in SEEDS[1:]:
        config = read(SEARCH)
        config["root_selection"]["seed"] = seed
        path = root / "configs" / f"search-{seed}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        write(path, config)
        search_configs[str(seed)] = sha(path)
    protocol = {
        "schema": "aic-hierarchical-target-learner-protocol-v1",
        "phase": "confirmation", "contrast": "success-selected",
        "experiment_commit": commit, "planned_records": 18,
        "runner_sha256": sha(Path(__file__)),
        "transform_runner_sha256": sha(Path(__file__).with_name(
            "hierarchical_success_imitation.py")),
        "helper_sha256": sha(Path(__file__).with_name(
            "hierarchical_target_learner.py")),
        "tracking_sha256": sha(Path(__file__).with_name(
            "hierarchical_target_learner_tracking.py")),
        "screen_protocol_sha256": sha(SCREEN_PROTOCOL),
        "screen_summary_sha256": sha(SCREEN_SUMMARY),
        "screen_diagnostic_sha256": reference_sha,
        "binary_sha256": sha(BINARY), "search_sha256": sha(SEARCH),
        "generation_sha256": sha(GENERATION),
        "sampling_sha256": sha(SAMPLING),
        "lower_sha256": sha(lower / "state.safetensors"),
        "outer_sha256": sha(outer / "state.safetensors"),
        "source_panels": {stream: sha(SOURCE / "inputs/panels" / f"{stream}.json")
                          for stream in STREAMS},
        "panels": panels, "search_configs": search_configs,
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
        "schema": "aic-hierarchical-success-onpolicy-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 streams x 3 generations x 2 arms; "
          "shared generation-zero collection; diagnostic=64")


def validate(root, require_observer=True):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    lower, outer = initial_models()
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["transform_runner_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_success_imitation.py"))
            and protocol["helper_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_target_learner.py"))
            and protocol["tracking_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_target_learner_tracking.py"))
            and protocol["screen_protocol_sha256"] == sha(SCREEN_PROTOCOL)
            and protocol["screen_summary_sha256"] == sha(SCREEN_SUMMARY)
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["search_sha256"] == sha(SEARCH)
            and protocol["generation_sha256"] == sha(GENERATION)
            and protocol["sampling_sha256"] == sha(SAMPLING)
            and protocol["lower_sha256"] == sha(lower / "state.safetensors")
            and protocol["outer_sha256"] == sha(outer / "state.safetensors")
            and protocol["screen_diagnostic_sha256"]
            == sha(root / "reference/panels/diagnostic.json"),
            "protocol input changed")
    for stream in STREAMS:
        require(protocol["source_panels"][stream]
                == sha(SOURCE / "inputs/panels" / f"{stream}.json"),
                "source panel changed")
        for generation in range(3):
            name = f"train-{stream}-g{generation}"
            require(protocol["panels"][name] == sha(panel_path(root, stream, generation)),
                    f"training panel changed: {name}")
    require(protocol["panels"]["diagnostic"]
            == sha(root / "panels/onpolicy-diagnostic.json"),
            "diagnostic panel changed")
    for seed in SEEDS[1:]:
        require(protocol["search_configs"][str(seed)]
                == sha(root / "configs" / f"search-{seed}.json"),
                "search stress configuration changed")
    if require_observer:
        observer = read(root / "wandb-observer.json")
        require(observer["protocol_sha256"] == sha(root / "protocol.json")
                and observer["status"] in ("ready", "running", "finished")
                and observer["remote_confirmed_step"] >= 0,
                "W&B readiness not remotely acknowledged")
    return protocol, progress


def evaluate(root, name, outer, config=SEARCH):
    lower, _ = initial_models()
    panel_file = root / "panels/onpolicy-diagnostic.json"
    output = (root / "evaluations" / f"{name}.json" if config == SEARCH else
              root / "stress-evaluations" / f"{name}.json")
    output.parent.mkdir(parents=True, exist_ok=True)
    if config == SEARCH:
        report, seconds = search(root, f"evaluate-{name}", panel_file, lower,
                                 outer, output)
    else:
        seconds = None
        if not output.exists():
            seconds = cli(root, f"stress-{name}", [
                "hierarchical-search-harness", "--config", config,
                "--panel", panel_file, "--lower-training-output", lower,
                "--outer-training-output", outer,
                "--catalog-root", "data/game/normalized", "--output", output,
                "--device", "cuda:0"])
        report = read(output)
        require(report["status"] == "complete"
                and report["panel_sha256"] == sha(panel_file)
                and report["lower_checkpoint_sha256"]
                == sha(lower / "state.safetensors")
                and report["outer_checkpoint_sha256"]
                == sha(outer / "state.safetensors"),
                "stress search lineage differs")
    cases = read(panel_file)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    return dict(outcomes(report, order, easy), report_sha256=sha(output),
                evaluation_seconds=seconds)


def train(root, name, panel_file, collection, outer):
    lower, _ = initial_models()
    output = root / "training" / name
    output.parent.mkdir(parents=True, exist_ok=True)
    if not output.exists():
        arguments = ["train-hierarchical-generation", "--config", GENERATION,
                     "--panel", panel_file, "--collection", collection,
                     "--lower-training-output", lower,
                     "--outer-training-output", outer,
                     "--catalog-root", "data/game/normalized",
                     "--output", output, "--device", "cuda:0"]
        if output.with_name(f".{output.name}.pending").exists():
            arguments.append("--resume")
        cli(root, f"train-{name}", arguments)
    report = read(output / "report.json")
    require(report["status"] == "complete" and report["epochs"] == 100
            and report["updates"] == 1600 and not report["train_lower"]
            and report["train_failed_policy"]
            and report["collection_sha256"] == sha(collection)
            and report["lower_checkpoint_sha256"]
            == sha(lower / "state.safetensors")
            and report["source_outer_checkpoint_sha256"]
            == sha(outer / "state.safetensors"),
            f"training lineage differs: {name}")
    return output, report


def run(root, max_records):
    protocol, progress = validate(root)
    lower, initial = initial_models()
    base = evaluate(root, "start", initial)
    if "baseline" not in progress:
        progress["baseline"] = base
        write(root / "progress.json", progress)
    else:
        require(progress["baseline"]["report_sha256"] == base["report_sha256"],
                "baseline changed")
    progress["status"] = "running"
    write(root / "progress.json", progress)
    added = 0
    for stream in STREAMS:
        for generation in range(3):
            source_panel = panel_path(root, stream, generation)
            for arm in ARMS:
                name = f"{stream}-g{generation}-{arm}"
                retained = next((row for row in progress["records"]
                                 if row["name"] == name), None)
                if retained is not None:
                    require(sha(checkpoint(root, stream, generation, arm)
                                / "state.safetensors")
                            == retained["outer_checkpoint_sha256"],
                            f"retained model changed: {name}")
                    continue
                if max_records is not None and added >= max_records:
                    return
                outer = checkpoint(root, stream, generation - 1, arm)
                if generation:
                    previous = next(row for row in progress["records"]
                                    if row["name"]
                                    == f"{stream}-g{generation-1}-{arm}")
                    require(sha(outer / "state.safetensors")
                            == previous["outer_checkpoint_sha256"],
                            f"previous model changed: {name}")
                source_name = (f"{stream}-g{generation}-current" if generation == 0
                               else name)
                source = root / "collections" / f"{source_name}-source.json"
                report, search_seconds = search(root, f"collect-{name}",
                                                source_panel, lower, outer, source)
                require(len(report["cases"]) == 32, "collection case count differs")
                if arm == "success-on":
                    collection = root / "collections" / f"{name}-success.json"
                    manifest = root / "manifests" / f"{name}-success.json"
                    if collection.exists() != manifest.exists():
                        collection.unlink(missing_ok=True)
                        manifest.unlink(missing_ok=True)
                    if not collection.exists():
                        changed_target(source, collection, manifest)
                    require(read(manifest)["source_sha256"] == sha(source)
                            and read(manifest)["output_sha256"] == sha(collection),
                            "target transform lineage differs")
                    manifest_sha = sha(manifest)
                else:
                    collection, manifest_sha = source, None
                output, training = train(root, name, source_panel, collection, outer)
                result = evaluate(root, name, output / "outer")
                row = dict(result, name=name, stream=stream, arm=arm,
                           generation=generation + 1,
                           gained_from_start=sum(a == "0" and b == "1" for a, b in
                                                 zip(base["case_success"],
                                                     result["case_success"])),
                           lost_from_start=sum(a == "1" and b == "0" for a, b in
                                               zip(base["case_success"],
                                                   result["case_success"])),
                           source_collection_sha256=sha(source),
                           collection_sha256=sha(collection),
                           transform_manifest_sha256=manifest_sha,
                           training_report_sha256=sha(output / "report.json"),
                           outer_checkpoint_sha256=training["outer_checkpoint_sha256"],
                           initial_outer_loss=training["initial_outer_loss"],
                           final_outer_loss=training["final_outer_loss"],
                           search_seconds=search_seconds,
                           training_seconds=training["elapsed_seconds"])
                progress["records"].append(row)
                write(root / "progress.json", progress)
                added += 1
                print(f"onpolicy={name} completed={row['completed']}/64 "
                      f"lost={row['lost_from_start']}", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "confirmation incomplete")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def stress(root, max_records):
    protocol, progress = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "generations incomplete")
    output = root / "stress-progress.json"
    state = read(output) if output.exists() else {
        "schema": "aic-hierarchical-success-onpolicy-stress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "status": "running", "records": [],
    }
    require(state["protocol_sha256"] == sha(root / "protocol.json")
            and state["progress_sha256"] == sha(root / "progress.json"),
            "stress source changed")
    added = 0
    for seed in SEEDS:
        config = SEARCH if seed == SEEDS[0] else root / "configs" / f"search-{seed}.json"
        for stream in ("start", *STREAMS):
            for arm in (("initial",) if stream == "start" else ARMS):
                name = f"{stream}-{arm}-{seed}"
                if any(row["name"] == name for row in state["records"]):
                    continue
                if max_records is not None and added >= max_records:
                    write(output, state)
                    return
                if seed == SEEDS[0]:
                    source = (progress["baseline"] if stream == "start" else
                              next(row for row in progress["records"] if row["name"]
                                   == f"{stream}-g2-{arm}"))
                    result = {key: source[key] for key in
                              ("case_success", "completed", "report_sha256")}
                else:
                    model = initial_models()[1] if stream == "start" else checkpoint(
                        root, stream, 2, arm)
                    result = evaluate(root, name, model, config)
                state["records"].append(dict(result, name=name, stream=stream,
                                             arm=arm, seed=seed))
                write(output, state)
                added += 1
                print(f"stress={name} completed={result['completed']}/64", flush=True)
    require(len(state["records"]) == len(SEEDS) * (1 + len(STREAMS) * len(ARMS)),
            "stress incomplete")
    state["status"] = "complete"
    write(output, state)


def summarize(root):
    protocol, progress = validate(root)
    state = read(root / "stress-progress.json")
    require(progress["status"] == state["status"] == "complete", "run incomplete")
    by = {(r["stream"], r["generation"], r["arm"]): r
          for r in progress["records"]}
    final_completion = [by[(s, 3, "success-on")]["completed"]
                        - by[(s, 3, "current")]["completed"] for s in STREAMS]
    final_retention = [by[(s, 3, "current")]["lost_from_start"]
                       - by[(s, 3, "success-on")]["lost_from_start"]
                       for s in STREAMS]
    later_completion = [by[(s, g, "success-on")]["completed"]
                        - by[(s, g, "current")]["completed"]
                        for s in STREAMS for g in (2, 3)]
    stress_by = {(r["seed"], r["stream"], r["arm"]): r
                 for r in state["records"]}
    seed_means = {str(seed): statistics.mean(
        stress_by[(seed, s, "success-on")]["completed"]
        - stress_by[(seed, s, "current")]["completed"] for s in STREAMS)
        for seed in SEEDS}
    gates = protocol["numeric_gate"]
    result = {
        "schema": "aic-hierarchical-success-onpolicy-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "stress_progress_sha256": sha(root / "stress-progress.json"),
        "final_completion_differences": final_completion,
        "final_retention_differences": final_retention,
        "later_completion_differences": later_completion,
        "seed_mean_completion_differences": seed_means,
        "passes_gate": (
            statistics.mean(final_completion) >= gates["final_mean_completion_gain"]
            and sum(x > 0 for x in final_completion) >= gates["final_stream_wins"]
            and statistics.mean(final_retention)
            >= gates["final_mean_fewer_lost_start_successes"]
            and sum(x > 0 for x in final_retention)
            >= gates["final_positive_retention_streams"]
            and statistics.mean(later_completion)
            >= gates["generation_2_3_mean_completion_gain"]
            and statistics.mean(seed_means.values())
            >= gates["all_seed_mean_completion_gain"]
            and sum(x > 0 for x in seed_means.values())
            >= gates["positive_seed_means"]),
    }
    write(root / "summary.json", result)
    print(f"final_completion={final_completion} retention={final_retention} "
          f"seed_means={seed_means} gate={result['passes_gate']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "stress", "summarize"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    elif args.command == "run":
        run(args.output, args.max_records)
    elif args.command == "stress":
        stress(args.output, args.max_records)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
