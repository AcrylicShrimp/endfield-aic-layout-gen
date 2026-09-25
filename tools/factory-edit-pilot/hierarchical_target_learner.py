"""Run the frozen-collection screen and fresh-collection target confirmation."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

from ppo_harness_state import read, sha, write
from transform_hierarchical_policy_target import transform


STREAMS = ("s1", "s2", "s3")
SEARCH = Path("docs/benchmarks/hierarchical-target-causal/c16-n128-search.json")
GENERATION = Path("docs/benchmarks/hierarchical-update-budget/generation-100.json")
SAMPLING = Path("docs/benchmarks/direct-constructive-full-graphs/sampling.json")
BINARY = Path("target/release/aic-cli")


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-target-learner: {message}")


def identity(case):
    payload = json.dumps(case["problem"], sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(payload.encode()).hexdigest()


def cli(root, name, arguments):
    log = root / "logs" / f"{name}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    with log.open("w") as output:
        result = subprocess.run([str(BINARY), "learning", *map(str, arguments)],
                                stdout=output, stderr=subprocess.STDOUT, check=False)
    require(result.returncode == 0, f"{name} failed ({result.returncode}); see {log}")
    return time.monotonic() - started


def search(root, name, panel, lower, outer, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    if not output.exists():
        seconds = cli(root, name, ["hierarchical-search-harness", "--config", SEARCH,
            "--panel", panel, "--lower-training-output", lower,
            "--outer-training-output", outer, "--catalog-root", "data/game/normalized",
            "--output", output, "--device", "cuda:0"])
    else:
        seconds = None
    report = read(output)
    require(report["status"] == "complete" and report["panel_sha256"] == sha(panel)
            and report["lower_checkpoint_sha256"] == sha(lower / "state.safetensors")
            and report["outer_checkpoint_sha256"] == sha(outer / "state.safetensors"),
            f"search lineage differs: {output}")
    return report, seconds


def panel(root, name, seed, candidates, take, seen):
    draft = root / "candidate-panels" / f"{name}.json"
    draft.parent.mkdir(parents=True, exist_ok=True)
    cli(root, f"prepare-{name}", ["prepare-hierarchical-panel", "--sampling", SAMPLING,
        "--catalog-root", "data/game/normalized", "--output", draft,
        "--partition", "training", "--cases", candidates, "--seed", seed,
        "--namespace", f"hierarchical-target-learner-{name}-v1",
        "--min-facilities", 2, "--max-facilities", 2, "--max-terminals", 6,
        "--max-connections", 6, "--max-turns", 10000, "--max-extension", 32])
    source = read(draft)
    selected = []
    for case in source["cases"]:
        key = identity(case)
        if key not in seen:
            selected.append(case)
            seen.add(key)
        if len(selected) == take:
            break
    require(len(selected) == take, f"too few disjoint cases: {name}")
    source["cases"] = selected
    output = root / "panels" / f"{name}.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    write(output, source)
    return sha(output)


def prepare(root):
    learner = root / "learner"
    require(not learner.exists(), "learner root already exists")
    continuation = read(root / "continuation-summary.json")
    contrast = read(root / "target-contrast-analysis.json")
    require(contrast["continuation_summary_sha256"] == sha(root / "continuation-summary.json")
            and contrast["selected_contrast"] in ("q2", "q4", "half-visited"),
            "continuation or target analysis is not final")
    config = read(GENERATION)
    require(config["epochs"] == 100 and config["outer_states_per_epoch"] == 256
            and config["batch_states"] == 16 and config["seed"] == 20260925
            and config["outer_learning_rate"] == 0.0003
            and not config["train_lower"] and config["train_failed_policy"],
            "learner configuration differs")
    require(read(SEARCH)["simulations_per_move"] == 128
            and read(SEARCH)["root_selection"]["candidates"] == 16,
            "search configuration differs")
    require(continuation["schema"] == "aic-hierarchical-first-action-summary-v1",
            "continuation summary differs")
    seen = set()
    for stream in STREAMS:
        source = read(root / "inputs/panels" / f"{stream}.json")
        require(len(source["cases"]) == 16, f"source panel differs: {stream}")
        for case in source["cases"]:
            require(identity(case) not in seen, "duplicate source problem")
            seen.add(identity(case))
    hashes = {}
    for index, stream in enumerate(STREAMS, start=1):
        for generation in range(3):
            name = f"retention-{stream}-g{generation}"
            hashes[name] = panel(learner, name, 20261800 + 100*index + generation,
                                 256, 32, seen)
    hashes["screen-diagnostic"] = panel(learner, "screen-diagnostic", 20262210,
                                         512, 64, seen)
    hashes["retention-diagnostic"] = panel(learner, "retention-diagnostic", 20262220,
                                            512, 64, seen)
    screen = {
        "schema": "aic-hierarchical-target-learner-protocol-v1", "phase": "screen",
        "contrast": contrast["selected_contrast"],
        "selection_reason": contrast["selection_reason"],
        "contrast_analysis_sha256": sha(root / "target-contrast-analysis.json"),
        "continuation_summary_sha256": sha(root / "continuation-summary.json"),
        "binary_sha256": sha(BINARY), "search_sha256": sha(SEARCH),
        "generation_sha256": sha(GENERATION), "sampling_sha256": sha(SAMPLING),
        "lower_sha256": sha(root / "inputs/models/lower/state.safetensors"),
        "outer_sha256": sha(root / "inputs/models/outer-128/state.safetensors"),
        "source_panel_sha256": {stream: sha(root / "inputs/panels" / f"{stream}.json")
                                for stream in STREAMS},
        "baseline_collection_sha256": {stream: sha(root / "raw" / stream / "c16-n128.json")
                                       for stream in STREAMS},
        "panels": hashes, "planned_records": 9,
    }
    (learner / "screen").mkdir(parents=True)
    write(learner / "screen/protocol.json", screen)
    write(learner / "screen/progress.json", {
        "schema": "aic-hierarchical-target-learner-progress-v1",
        "protocol_sha256": sha(learner / "screen/protocol.json"),
        "status": "prepared", "records": []})
    print(f"prepared_learner_panels={len(hashes)} disjoint_problems={len(seen)}")


def validated(root, phase):
    learner = root / "learner"
    directory = learner / phase
    protocol = read(directory / "protocol.json")
    progress = read(directory / "progress.json")
    require(progress["protocol_sha256"] == sha(directory / "protocol.json")
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["search_sha256"] == sha(SEARCH)
            and protocol["generation_sha256"] == sha(GENERATION)
            and protocol["lower_sha256"] == sha(root / "inputs/models/lower/state.safetensors")
            and protocol["outer_sha256"] == sha(root / "inputs/models/outer-128/state.safetensors"),
            "protocol input changed")
    for name, expected in protocol["panels"].items():
        require(sha(learner / "panels" / f"{name}.json") == expected,
                f"panel changed: {name}")
    observer = read(directory / "wandb-observer.json")
    require(observer["protocol_sha256"] == sha(directory / "protocol.json")
            and observer["status"] in ("ready", "running")
            and observer["remote_confirmed_step"] >= 0,
            "W&B readiness is not acknowledged")
    return learner, directory, protocol, progress


def outcomes(report, order, easy):
    by_id = {case["id"]: case["mcts"] for case in report["cases"]}
    require(set(by_id) == set(order), "diagnostic cases changed")
    success = [bool(by_id[key]["fully_validated"]) for key in order]
    return {
        "case_success": "".join("1" if value else "0" for value in success),
        "completed": sum(success),
        "easy_completed": sum(value and flag for value, flag in zip(success, easy)),
        "hard_completed": sum(value and not flag for value, flag in zip(success, easy)),
        "termination_counts": {name: sum(by_id[key]["termination"] == name for key in order)
                               for name in sorted({by_id[key]["termination"] for key in order})},
        "completed_connection_sum": sum(by_id[key]["completed_connections"] for key in order),
        "validated_footprint_mean": statistics.mean(
            by_id[key]["footprint_area"] for key in order if by_id[key]["fully_validated"])
            if any(success) else None,
        "validated_transport_mean": statistics.mean(
            by_id[key]["transport_cells"] for key in order if by_id[key]["fully_validated"])
            if any(success) else None,
    }


def diagnostic(root, directory, name, panel_name, lower, outer):
    source = root / "learner/panels" / f"{panel_name}.json"
    output = directory / "evaluations" / f"{name}.json"
    report, seconds = search(directory, f"evaluate-{name}", source, lower, outer, output)
    panel = read(source)
    order = [case["id"] for case in panel["cases"]]
    easy = [len(case["problem"]["connections"]) <= 4 for case in panel["cases"]]
    return dict(outcomes(report, order, easy), report_sha256=sha(output),
                evaluation_seconds=seconds)


def trained(root, directory, name, panel_path, collection, lower, outer):
    output = directory / "training" / name
    output.parent.mkdir(parents=True, exist_ok=True)
    if not output.exists():
        arguments = ["train-hierarchical-generation", "--config", GENERATION,
            "--panel", panel_path, "--collection", collection,
            "--lower-training-output", lower, "--outer-training-output", outer,
            "--catalog-root", "data/game/normalized", "--output", output,
            "--device", "cuda:0"]
        if output.with_name(f".{output.name}.pending").exists():
            arguments.append("--resume")
        cli(directory, f"train-{name}", arguments)
    result = read(output / "report.json")
    require(result["status"] == "complete" and result["epochs"] == 100
            and result["updates"] == 1600 and not result["train_lower"]
            and result["train_failed_policy"]
            and result["collection_sha256"] == sha(collection)
            and result["lower_checkpoint_sha256"] == sha(lower / "state.safetensors")
            and result["source_outer_checkpoint_sha256"] == sha(outer / "state.safetensors"),
            f"training lineage differs: {name}")
    return output, result


def transformed(directory, name, source, contrast, scope):
    output = directory / "collections" / f"{name}.json"
    manifest = directory / "manifests" / f"{name}.json"
    if not output.exists() and not manifest.exists():
        transform(source, output, manifest, contrast, scope)
    record = read(manifest)
    require(record["source_sha256"] == sha(source)
            and record["output_sha256"] == sha(output)
            and record["variant"] == contrast and record["scope"] == scope,
            f"target transform lineage differs: {name}")
    return output, sha(manifest)


def append(directory, progress, row):
    progress["records"].append(row)
    progress["status"] = "running"
    write(directory / "progress.json", progress)


def screen(root, max_records):
    learner, directory, protocol, progress = validated(root, "screen")
    lower = root / "inputs/models/lower"
    initial = root / "inputs/models/outer-128"
    base = diagnostic(root, directory, "start", "screen-diagnostic", lower, initial)
    if "baseline" not in progress:
        progress["baseline"] = base
        write(directory / "progress.json", progress)
    else:
        require(progress["baseline"]["case_success"] == base["case_success"]
                and progress["baseline"]["report_sha256"] == base["report_sha256"],
                "screen baseline changed")
    added = 0
    for stream in STREAMS:
        panel_path = root / "inputs/panels" / f"{stream}.json"
        source = root / "raw" / stream / "c16-n128.json"
        require(sha(source) == protocol["baseline_collection_sha256"][stream],
                f"frozen collection changed: {stream}")
        for arm in ("current", "contrast-first", "contrast-all"):
            name = f"{stream}-{arm}"
            retained = next((row for row in progress["records"] if row["name"] == name), None)
            if retained is not None:
                require(sha(directory / "training" / name / "outer/state.safetensors")
                        == retained["outer_checkpoint_sha256"],
                        f"retained screen model changed: {name}")
                continue
            if max_records is not None and added >= max_records:
                return
            collection, manifest_sha = (source, None) if arm == "current" else transformed(
                directory, name, source, protocol["contrast"], arm.split("-")[1])
            output, trained_report = trained(root, directory, name, panel_path,
                                             collection, lower, initial)
            result = diagnostic(root, directory, name, "screen-diagnostic",
                                output / "lower", output / "outer")
            row = dict(result, name=name, stream=stream, arm=arm,
                gained_from_start=sum(a == "0" and b == "1" for a, b in zip(
                    base["case_success"], result["case_success"])),
                lost_from_start=sum(a == "1" and b == "0" for a, b in zip(
                    base["case_success"], result["case_success"])),
                collection_sha256=sha(collection), transform_manifest_sha256=manifest_sha,
                training_report_sha256=sha(output / "report.json"),
                outer_checkpoint_sha256=trained_report["outer_checkpoint_sha256"],
                initial_outer_loss=trained_report["initial_outer_loss"],
                final_outer_loss=trained_report["final_outer_loss"],
                training_seconds=trained_report["elapsed_seconds"])
            append(directory, progress, row)
            added += 1
            print(f"screen={name} completed={row['completed']}/64 "
                  f"lost={row['lost_from_start']}", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "screen missing records")
    progress["status"] = "complete"
    write(directory / "progress.json", progress)


def choose_scope(root):
    directory = root / "learner/screen"
    progress = read(directory / "progress.json")
    require(progress["status"] == "complete" and len(progress["records"]) == 9,
            "screen incomplete")
    observer = read(directory / "wandb-observer.json")
    require(observer["status"] == "finished"
            and observer["remote_confirmed_step"] >= 9,
            "screen W&B closeout is incomplete")
    by = {(row["stream"], row["arm"]): row for row in progress["records"]}
    calculations = {}
    eligible = []
    for scope in ("first", "all"):
        differences = [(by[(s, "current")]["lost_from_start"]
                        - by[(s, f"contrast-{scope}")]["lost_from_start"])
                       for s in STREAMS]
        completions = [(by[(s, f"contrast-{scope}")]["completed"]
                        - by[(s, "current")]["completed"]) for s in STREAMS]
        means = (statistics.mean(differences), statistics.mean(completions))
        passing = means[0] >= 3 and sum(value > 0 for value in differences) >= 2 \
            and means[1] >= -2
        calculations[scope] = {"retention_differences": differences,
                               "completion_differences": completions,
                               "mean_retention_difference": means[0],
                               "mean_completion_difference": means[1],
                               "passed": passing}
        if passing:
            eligible.append((-means[0], -means[1], 0 if scope == "first" else 1, scope))
    scope = min(eligible)[3] if eligible else "first"
    return scope, "screen-gate" if eligible else "stress-contrast-only", calculations


def prepare_confirmation(root):
    learner = root / "learner"
    directory = learner / "confirmation"
    require(not directory.exists(), "confirmation already prepared")
    scope, reason, calculations = choose_scope(root)
    screen_protocol = read(learner / "screen/protocol.json")
    protocol = {
        "schema": "aic-hierarchical-target-learner-protocol-v1", "phase": "confirmation",
        "contrast": screen_protocol["contrast"], "scope": scope,
        "selection_reason": reason, "scope_calculations": calculations,
        "screen_progress_sha256": sha(learner / "screen/progress.json"),
        "binary_sha256": sha(BINARY), "search_sha256": sha(SEARCH),
        "generation_sha256": sha(GENERATION),
        "lower_sha256": screen_protocol["lower_sha256"],
        "outer_sha256": screen_protocol["outer_sha256"],
        "panels": screen_protocol["panels"], "planned_records": 18,
    }
    directory.mkdir(parents=True)
    write(directory / "protocol.json", protocol)
    write(directory / "progress.json", {
        "schema": "aic-hierarchical-target-learner-progress-v1",
        "protocol_sha256": sha(directory / "protocol.json"),
        "status": "prepared", "records": []})
    print(f"confirmation_scope={scope} reason={reason}")


def confirmation(root, max_records):
    learner, directory, protocol, progress = validated(root, "confirmation")
    require(sha(learner / "screen/progress.json") == protocol["screen_progress_sha256"],
            "screen closeout changed")
    lower = root / "inputs/models/lower"
    initial = root / "inputs/models/outer-128"
    base = diagnostic(root, directory, "start", "retention-diagnostic", lower, initial)
    if "baseline" not in progress:
        progress["baseline"] = base
        write(directory / "progress.json", progress)
    else:
        require(progress["baseline"]["case_success"] == base["case_success"]
                and progress["baseline"]["report_sha256"] == base["report_sha256"],
                "confirmation baseline changed")
    added = 0
    for stream in STREAMS:
        for generation in range(3):
            panel_path = learner / "panels" / f"retention-{stream}-g{generation}.json"
            for arm in ("current", "contrast"):
                name = f"{stream}-g{generation}-{arm}"
                retained = next((row for row in progress["records"] if row["name"] == name), None)
                if retained is not None:
                    require(sha(directory / "training" / name / "outer/state.safetensors")
                            == retained["outer_checkpoint_sha256"],
                            f"retained confirmation model changed: {name}")
                    continue
                if max_records is not None and added >= max_records:
                    return
                outer = initial if generation == 0 else (directory / "training" /
                    f"{stream}-g{generation-1}-{arm}" / "outer")
                if generation > 0:
                    previous = next(row for row in progress["records"]
                                    if row["name"] == f"{stream}-g{generation-1}-{arm}")
                    require(sha(outer / "state.safetensors")
                            == previous["outer_checkpoint_sha256"],
                            f"previous model changed: {stream}/{generation}/{arm}")
                source = directory / "collections" / f"{stream}-g{generation}-{arm}-source.json"
                if generation == 0 and arm == "contrast":
                    source = directory / "collections" / f"{stream}-g0-current-source.json"
                report, search_seconds = search(directory, f"collect-{name}",
                                                panel_path, lower, outer, source)
                collection, manifest_sha = (source, None) if arm == "current" else transformed(
                    directory, name, source, protocol["contrast"], protocol["scope"])
                output, trained_report = trained(root, directory, name, panel_path,
                                                 collection, lower, outer)
                result = diagnostic(root, directory, name, "retention-diagnostic",
                                    output / "lower", output / "outer")
                row = dict(result, name=name, stream=stream, arm=arm,
                    generation=generation + 1,
                    gained_from_start=sum(a == "0" and b == "1" for a, b in zip(
                        base["case_success"], result["case_success"])),
                    lost_from_start=sum(a == "1" and b == "0" for a, b in zip(
                        base["case_success"], result["case_success"])),
                    source_collection_sha256=sha(source),
                    collection_sha256=sha(collection),
                    transform_manifest_sha256=manifest_sha,
                    training_report_sha256=sha(output / "report.json"),
                    outer_checkpoint_sha256=trained_report["outer_checkpoint_sha256"],
                    initial_outer_loss=trained_report["initial_outer_loss"],
                    final_outer_loss=trained_report["final_outer_loss"],
                    search_seconds=search_seconds,
                    training_seconds=trained_report["elapsed_seconds"])
                append(directory, progress, row)
                added += 1
                print(f"confirmation={name} completed={row['completed']}/64 "
                      f"lost={row['lost_from_start']}", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "confirmation missing records")
    progress["status"] = "complete"
    write(directory / "progress.json", progress)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "screen", "prepare-confirmation",
                                            "confirmation"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    os.chdir(Path(__file__).resolve().parents[2])
    if args.command == "prepare":
        prepare(args.output)
    elif args.command == "screen":
        screen(args.output, args.max_records)
    elif args.command == "prepare-confirmation":
        prepare_confirmation(args.output)
    else:
        confirmation(args.output, args.max_records)


if __name__ == "__main__":
    main()
