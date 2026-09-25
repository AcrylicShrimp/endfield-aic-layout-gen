"""Check horizon-12 final models under three paired root-search seeds."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import statistics

from hierarchical_horizon_onpolicy import (CONTROL_PROGRESS, HORIZON, SEED,
                                            checkpoint, control, initial_models,
                                            require, search_config, validate)
from hierarchical_success_onpolicy import SEEDS
from hierarchical_target_learner import BINARY, SEARCH, STREAMS, cli, outcomes
from ppo_harness_state import read, sha, write


CONTROL_MODELS = Path("artifacts/recovery-success-onpolicy-final-models")


def source_model(root, stream, horizon):
    if stream == "start":
        return initial_models()[1] if horizon == 6 else checkpoint(root, STREAMS[0], -1)
    return (CONTROL_MODELS / f"{stream}-g2-current" if horizon == 6
            else checkpoint(root, stream, 2))


def seeded_search(root, horizon, seed):
    source = read(search_config(root) if horizon == HORIZON else SEARCH)
    require(source["horizon"] == horizon
            and source["root_selection"]["seed"] == SEED,
            "base search configuration changed")
    source["root_selection"]["seed"] = seed
    return source


def semantic_hash(config):
    return hashlib.sha256(json.dumps(config, separators=(",", ":")).encode()).hexdigest()


def prepare(root):
    protocol, progress, old = validate(root)
    primary = read(root / "primary-summary.json")
    observer = read(root / "wandb-observer.json")
    require(progress["status"] == "complete" and primary["passes_primary_gate"]
            and primary["progress_sha256"] == sha(root / "progress.json")
            and observer["status"] == "finished"
            and observer["exported_records"] == protocol["planned_records"],
            "primary gate not passed")
    stress = root / "stress"
    require(not stress.exists(), "stress output already exists")
    lower, _ = initial_models()
    stress.mkdir()
    model_hashes = {}
    for stream in ("start", *STREAMS):
        for horizon in (6, HORIZON):
            model = source_model(root, stream, horizon)
            key = f"{stream}-h{horizon}"
            model_hashes[key] = sha(model / "state.safetensors")
            if stream != "start":
                expected = (next(r for r in old["records"] if r["name"]
                                 == f"{stream}-g2-current")["outer_checkpoint_sha256"]
                            if horizon == 6 else next(r for r in progress["records"]
                                                        if r["name"] == f"{stream}-g2-h12")[
                                                            "outer_checkpoint_sha256"])
                require(model_hashes[key] == expected, f"final model changed: {key}")
            for seed in SEEDS[1:]:
                search = seeded_search(root, horizon, seed)
                config = stress / "configs" / f"search-h{horizon}-{seed}.json"
                config.parent.mkdir(parents=True, exist_ok=True)
                if not config.exists():
                    write(config, search)
                shadow = stress / "models" / f"{key}-{seed}"
                shadow.mkdir(parents=True)
                for filename in ("state.safetensors", "parameter-order.json"):
                    shutil.copy2(model / filename, shadow / filename)
                outer_config = read(model / "config.json")
                require(outer_config["search"]["horizon"] == horizon,
                        f"model search contract differs: {key}")
                outer_config["search"] = search
                write(shadow / "config.json", outer_config)
    write(stress / "protocol.json", {
        "schema": "aic-hierarchical-horizon-seed-stress-protocol-v1",
        "runner_sha256": sha(Path(__file__)),
        "parent_protocol_sha256": sha(root / "protocol.json"),
        "parent_progress_sha256": sha(root / "progress.json"),
        "primary_summary_sha256": sha(root / "primary-summary.json"),
        "control_progress_sha256": sha(CONTROL_PROGRESS),
        "binary_sha256": sha(BINARY),
        "lower_sha256": sha(lower / "state.safetensors"),
        "panel_sha256": sha(root / "panels/onpolicy-diagnostic.json"),
        "model_sha256": model_hashes,
        "seeds": list(SEEDS),
        "gate": {name: protocol["numeric_gate"][name] for name in
                 ("all_seed_mean_completion_gain", "positive_seed_means")},
    })
    write(stress / "progress.json", {
        "schema": "aic-hierarchical-horizon-seed-stress-progress-v1",
        "protocol_sha256": sha(stress / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 seeds x 4 model origins x 2 search horizons")


def validate_stress(root):
    _, progress, old = validate(root)
    stress = root / "stress"
    protocol = read(stress / "protocol.json")
    state = read(stress / "progress.json")
    lower, _ = initial_models()
    require(state["protocol_sha256"] == sha(stress / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["parent_protocol_sha256"] == sha(root / "protocol.json")
            and protocol["parent_progress_sha256"] == sha(root / "progress.json")
            and protocol["primary_summary_sha256"] == sha(root / "primary-summary.json")
            and protocol["control_progress_sha256"] == sha(CONTROL_PROGRESS)
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["lower_sha256"] == sha(lower / "state.safetensors")
            and protocol["panel_sha256"] == sha(root / "panels/onpolicy-diagnostic.json")
            and protocol["seeds"] == list(SEEDS), "stress input changed")
    for stream in ("start", *STREAMS):
        for horizon in (6, HORIZON):
            key = f"{stream}-h{horizon}"
            model = source_model(root, stream, horizon)
            require(protocol["model_sha256"][key] == sha(model / "state.safetensors"),
                    f"model changed: {key}")
            for seed in SEEDS[1:]:
                config = stress / "configs" / f"search-h{horizon}-{seed}.json"
                shadow = stress / "models" / f"{key}-{seed}"
                require(read(config) == seeded_search(root, horizon, seed)
                        and read(shadow / "config.json")["search"] == read(config)
                        and sha(shadow / "state.safetensors")
                        == protocol["model_sha256"][key],
                        f"seeded model contract changed: {key}-{seed}")
    return protocol, state, progress, old


def run(root, max_records):
    protocol, state, progress, old = validate_stress(root)
    stress = root / "stress"
    lower, _ = initial_models()
    source_panel = root / "panels/onpolicy-diagnostic.json"
    cases = read(source_panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    added = 0
    state["status"] = "running"
    write(stress / "progress.json", state)
    for seed in SEEDS:
        for stream in ("start", *STREAMS):
            for horizon in (6, HORIZON):
                name = f"{stream}-h{horizon}-{seed}"
                if any(r["name"] == name for r in state["records"]):
                    continue
                if max_records is not None and added >= max_records:
                    return
                if seed == SEED:
                    source = (old["baseline"] if stream == "start" and horizon == 6
                              else progress["initial_h12"] if stream == "start"
                              else next(r for r in (old if horizon == 6 else progress)["records"]
                                        if r["name"] == f"{stream}-g2-"
                                        f"{'current' if horizon == 6 else 'h12'}"))
                    result = {key: source[key] for key in
                              ("case_success", "completed", "report_sha256")}
                    seconds = None
                else:
                    config = stress / "configs" / f"search-h{horizon}-{seed}.json"
                    model = stress / "models" / f"{stream}-h{horizon}-{seed}"
                    output = stress / "evaluations" / f"{name}.json"
                    output.parent.mkdir(parents=True, exist_ok=True)
                    seconds = None
                    if not output.exists():
                        seconds = cli(stress, name, [
                            "hierarchical-search-harness", "--config", config,
                            "--panel", source_panel, "--lower-training-output", lower,
                            "--outer-training-output", model,
                            "--catalog-root", "data/game/normalized",
                            "--output", output, "--device", "cuda:0"])
                    report = read(output)
                    require(report["status"] == "complete"
                            and report["panel_sha256"] == protocol["panel_sha256"]
                            and report["search_config_sha256"]
                            == semantic_hash(read(config))
                            and report["lower_checkpoint_sha256"]
                            == protocol["lower_sha256"]
                            and report["outer_checkpoint_sha256"]
                            == protocol["model_sha256"][f"{stream}-h{horizon}"],
                            f"stress lineage differs: {name}")
                    result = {key: value for key, value in
                              outcomes(report, order, easy).items() if key in
                              ("case_success", "completed")}
                    result["report_sha256"] = sha(output)
                state["records"].append(dict(result, name=name, seed=seed,
                                             stream=stream, horizon=horizon,
                                             evaluation_seconds=seconds))
                write(stress / "progress.json", state)
                added += 1
                print(f"stress={name} completed={result['completed']}/64", flush=True)
    require(len(state["records"]) == len(SEEDS) * 8, "stress incomplete")
    state["status"] = "complete"
    write(stress / "progress.json", state)


def summarize(root):
    protocol, state, _, _ = validate_stress(root)
    require(state["status"] == "complete" and len(state["records"]) == 24,
            "stress incomplete")
    rows = {(r["seed"], r["stream"], r["horizon"]): r for r in state["records"]}
    seed_means = {str(seed): statistics.mean(
        rows[(seed, stream, HORIZON)]["completed"]
        - rows[(seed, stream, 6)]["completed"] for stream in STREAMS)
        for seed in SEEDS}
    gate = protocol["gate"]
    result = {
        "schema": "aic-hierarchical-horizon-seed-stress-summary-v1",
        "protocol_sha256": sha(root / "stress/protocol.json"),
        "progress_sha256": sha(root / "stress/progress.json"),
        "seed_mean_completion_differences": seed_means,
        "stream_completion_differences": {
            str(seed): {stream: rows[(seed, stream, HORIZON)]["completed"]
                        - rows[(seed, stream, 6)]["completed"] for stream in STREAMS}
            for seed in SEEDS},
        "starting_completions": {
            str(seed): {f"h{horizon}": rows[(seed, "start", horizon)]["completed"]
                        for horizon in (6, HORIZON)} for seed in SEEDS},
        "mean_completion_difference": statistics.mean(seed_means.values()),
        "positive_seed_means": sum(value > 0 for value in seed_means.values()),
        "passes_stress_gate": (statistics.mean(seed_means.values())
                               >= gate["all_seed_mean_completion_gain"]
                               and sum(value > 0 for value in seed_means.values())
                               >= gate["positive_seed_means"]),
    }
    write(root / "stress/summary.json", result)
    print(f"seed_means={seed_means} passes_stress_gate={result['passes_stress_gate']}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.output)
    elif args.command == "run":
        run(args.output, args.max_records)
    else:
        summarize(args.output)
