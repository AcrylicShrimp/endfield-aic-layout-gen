"""Separate learned-model and search-horizon effects on paired diagnostic cases."""

import argparse
from pathlib import Path
import shutil
import statistics

from hierarchical_horizon_onpolicy import (CONTROL_PROGRESS, HORIZON, checkpoint,
                                            initial_models, require)
from hierarchical_horizon_stress import (CONTROL_MODELS, semantic_hash,
                                         validate_stress)
from hierarchical_target_learner import BINARY, SEARCH, STREAMS, cli, outcomes
from ppo_harness_state import read, sha, write


def model(root, stream, trained_horizon):
    return (CONTROL_MODELS / f"{stream}-g2-current" if trained_horizon == 6
            else checkpoint(root, stream, 2))


def search(root, search_horizon):
    return SEARCH if search_horizon == 6 else root / "configs/search-h12.json"


def name(stream, trained_horizon, search_horizon):
    return f"{stream}-model-h{trained_horizon}-search-h{search_horizon}"


def prepare(root):
    stress_protocol, stress_state, _, _ = validate_stress(root)
    stress_summary = read(root / "stress/summary.json")
    require(stress_state["status"] == "complete"
            and stress_summary["passes_stress_gate"]
            and stress_summary["protocol_sha256"] == sha(root / "stress/protocol.json")
            and stress_summary["progress_sha256"] == sha(root / "stress/progress.json"),
            "seed stress did not pass")
    cross = root / "cross"
    require(not cross.exists(), "cross output already exists")
    cross.mkdir()
    model_hashes = {}
    config_hashes = {}
    for stream in STREAMS:
        for trained_horizon, search_horizon in ((6, HORIZON), (HORIZON, 6)):
            key = name(stream, trained_horizon, search_horizon)
            source = model(root, stream, trained_horizon)
            model_hashes[key] = sha(source / "state.safetensors")
            require(model_hashes[key] == stress_protocol["model_sha256"][
                f"{stream}-h{trained_horizon}"], f"source model changed: {key}")
            shadow = cross / "models" / key
            shadow.mkdir(parents=True)
            for filename in ("state.safetensors", "parameter-order.json"):
                shutil.copy2(source / filename, shadow / filename)
            source_config = read(source / "config.json")
            require(source_config["search"]["horizon"] == trained_horizon,
                    f"training model contract changed: {key}")
            source_config["search"] = read(search(root, search_horizon))
            write(shadow / "config.json", source_config)
            config_hashes[key] = sha(shadow / "config.json")
    lower, _ = initial_models()
    write(cross / "protocol.json", {
        "schema": "aic-hierarchical-horizon-cross-protocol-v1",
        "runner_sha256": sha(Path(__file__)),
        "parent_protocol_sha256": sha(root / "protocol.json"),
        "parent_progress_sha256": sha(root / "progress.json"),
        "control_progress_sha256": sha(CONTROL_PROGRESS),
        "stress_protocol_sha256": sha(root / "stress/protocol.json"),
        "stress_summary_sha256": sha(root / "stress/summary.json"),
        "binary_sha256": sha(BINARY),
        "lower_sha256": sha(lower / "state.safetensors"),
        "panel_sha256": sha(root / "panels/onpolicy-diagnostic.json"),
        "search_sha256": {str(h): sha(search(root, h)) for h in (6, HORIZON)},
        "model_sha256": model_hashes,
        "shadow_config_sha256": config_hashes,
        "planned_records": 6,
    })
    write(cross / "progress.json", {
        "schema": "aic-hierarchical-horizon-cross-progress-v1",
        "protocol_sha256": sha(cross / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 streams x 2 crossed model/search settings")


def validate(root):
    stress_protocol, stress_state, progress, old = validate_stress(root)
    cross = root / "cross"
    protocol = read(cross / "protocol.json")
    state = read(cross / "progress.json")
    lower, _ = initial_models()
    require(stress_state["status"] == "complete"
            and state["protocol_sha256"] == sha(cross / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["parent_protocol_sha256"] == sha(root / "protocol.json")
            and protocol["parent_progress_sha256"] == sha(root / "progress.json")
            and protocol["control_progress_sha256"] == sha(CONTROL_PROGRESS)
            and protocol["stress_protocol_sha256"] == sha(root / "stress/protocol.json")
            and protocol["stress_summary_sha256"] == sha(root / "stress/summary.json")
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["lower_sha256"] == sha(lower / "state.safetensors")
            and protocol["panel_sha256"] == sha(root / "panels/onpolicy-diagnostic.json")
            and protocol["planned_records"] == 6,
            "cross input changed")
    for h in (6, HORIZON):
        require(protocol["search_sha256"][str(h)] == sha(search(root, h)),
                f"search configuration changed: {h}")
    for stream in STREAMS:
        for trained_horizon, search_horizon in ((6, HORIZON), (HORIZON, 6)):
            key = name(stream, trained_horizon, search_horizon)
            shadow = cross / "models" / key
            require(protocol["model_sha256"][key]
                    == stress_protocol["model_sha256"][f"{stream}-h{trained_horizon}"]
                    == sha(model(root, stream, trained_horizon) / "state.safetensors")
                    == sha(shadow / "state.safetensors")
                    and protocol["shadow_config_sha256"][key]
                    == sha(shadow / "config.json")
                    and read(shadow / "config.json")["search"]
                    == read(search(root, search_horizon)),
                    f"cross model contract changed: {key}")
    return protocol, state, progress, old


def run(root, max_records):
    protocol, state, _, _ = validate(root)
    cross = root / "cross"
    lower, _ = initial_models()
    panel = root / "panels/onpolicy-diagnostic.json"
    cases = read(panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    state["status"] = "running"
    write(cross / "progress.json", state)
    added = 0
    for stream in STREAMS:
        for trained_horizon, search_horizon in ((6, HORIZON), (HORIZON, 6)):
            key = name(stream, trained_horizon, search_horizon)
            if any(row["name"] == key for row in state["records"]):
                continue
            if max_records is not None and added >= max_records:
                return
            output = cross / "evaluations" / f"{key}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            seconds = None
            if not output.exists():
                seconds = cli(cross, key, [
                    "hierarchical-search-harness", "--config",
                    search(root, search_horizon), "--panel", panel,
                    "--lower-training-output", lower, "--outer-training-output",
                    cross / "models" / key, "--catalog-root", "data/game/normalized",
                    "--output", output, "--device", "cuda:0"])
            report = read(output)
            require(report["status"] == "complete"
                    and report["panel_sha256"] == protocol["panel_sha256"]
                    and report["search_config_sha256"]
                    == semantic_hash(read(search(root, search_horizon)))
                    and report["lower_checkpoint_sha256"] == protocol["lower_sha256"]
                    and report["outer_checkpoint_sha256"]
                    == protocol["model_sha256"][key],
                    f"cross evaluation lineage differs: {key}")
            result = {k: v for k, v in outcomes(report, order, easy).items()
                      if k in ("case_success", "completed")}
            state["records"].append(dict(result, name=key, stream=stream,
                                         trained_horizon=trained_horizon,
                                         search_horizon=search_horizon,
                                         report_sha256=sha(output),
                                         evaluation_seconds=seconds))
            write(cross / "progress.json", state)
            added += 1
            print(f"cross={key} completed={result['completed']}/64", flush=True)
    require(len(state["records"]) == protocol["planned_records"],
            "cross evaluation incomplete")
    state["status"] = "complete"
    write(cross / "progress.json", state)


def summarize(root):
    protocol, state, progress, old = validate(root)
    require(state["status"] == "complete"
            and len(state["records"]) == protocol["planned_records"],
            "cross evaluation incomplete")
    baseline = old["baseline"]["case_success"]
    rows = {}
    for stream in STREAMS:
        for trained_horizon in (6, HORIZON):
            for search_horizon in (6, HORIZON):
                if trained_horizon == search_horizon:
                    source = old if trained_horizon == 6 else progress
                    row = next(r for r in source["records"] if r["name"]
                               == f"{stream}-g2-"
                               f"{'current' if trained_horizon == 6 else 'h12'}")
                else:
                    row = next(r for r in state["records"] if r["name"]
                               == name(stream, trained_horizon, search_horizon))
                rows[(stream, trained_horizon, search_horizon)] = {
                    "completed": row["completed"],
                    "lost_start_successes": sum(a == "1" and b == "0" for a, b in
                                                zip(baseline, row["case_success"])),
                    "case_success": row["case_success"],
                }
    completion_matrix = {stream: {
        f"model-h{m}-search-h{s}": rows[(stream, m, s)]["completed"]
        for m in (6, HORIZON) for s in (6, HORIZON)} for stream in STREAMS}
    lost_matrix = {stream: {
        f"model-h{m}-search-h{s}": rows[(stream, m, s)]["lost_start_successes"]
        for m in (6, HORIZON) for s in (6, HORIZON)} for stream in STREAMS}
    model_effects = {str(s): [rows[(stream, HORIZON, s)]["completed"]
                              - rows[(stream, 6, s)]["completed"]
                              for stream in STREAMS] for s in (6, HORIZON)}
    search_effects = {str(m): [rows[(stream, m, HORIZON)]["completed"]
                               - rows[(stream, m, 6)]["completed"]
                               for stream in STREAMS] for m in (6, HORIZON)}
    result = {
        "schema": "aic-hierarchical-horizon-cross-summary-v1",
        "protocol_sha256": sha(root / "cross/protocol.json"),
        "progress_sha256": sha(root / "cross/progress.json"),
        "completion_matrix": completion_matrix,
        "lost_start_successes_matrix": lost_matrix,
        "model_completion_effects_by_search_horizon": model_effects,
        "model_effect_means": {s: statistics.mean(v) for s, v in model_effects.items()},
        "search_completion_effects_by_model_horizon": search_effects,
        "search_effect_means": {m: statistics.mean(v) for m, v in search_effects.items()},
    }
    write(root / "cross/summary.json", result)
    print(f"model_effect_means={result['model_effect_means']} "
          f"search_effect_means={result['search_effect_means']}")


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
