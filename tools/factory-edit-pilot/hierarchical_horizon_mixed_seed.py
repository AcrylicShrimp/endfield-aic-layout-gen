"""Confirm H6-trained weights with H12 search under new root seeds."""

import argparse
from pathlib import Path
import shutil
import statistics

from hierarchical_horizon_cross import validate as validate_cross
from hierarchical_horizon_onpolicy import HORIZON, initial_models, require
from hierarchical_horizon_stress import (semantic_hash, seeded_search,
                                         source_model)
from hierarchical_success_onpolicy import SEEDS
from hierarchical_target_learner import BINARY, STREAMS, cli, outcomes
from ppo_harness_state import read, sha, write


NEW_SEEDS = SEEDS[1:]


def name(stream, seed):
    return f"{stream}-h6-model-h12-search-{seed}"


def prepare(root):
    cross_protocol, cross_state, _, _ = validate_cross(root)
    stress = read(root / "stress/progress.json")
    stress_summary = read(root / "stress/summary.json")
    require(cross_state["status"] == "complete"
            and len(cross_state["records"]) == cross_protocol["planned_records"]
            and stress["status"] == "complete" and stress_summary["passes_stress_gate"],
            "source studies incomplete")
    mixed = root / "mixed-seed"
    require(not mixed.exists(), "mixed-seed output already exists")
    mixed.mkdir()
    models = {}
    orders = {}
    configurations = {}
    for stream in STREAMS:
        source = source_model(root, stream, 6)
        models[stream] = sha(source / "state.safetensors")
        orders[stream] = sha(source / "parameter-order.json")
        for seed in NEW_SEEDS:
            key = name(stream, seed)
            config = mixed / "configs" / f"search-h12-{seed}.json"
            config.parent.mkdir(parents=True, exist_ok=True)
            if not config.exists():
                write(config, seeded_search(root, HORIZON, seed))
            shadow = mixed / "models" / key
            shadow.mkdir(parents=True)
            for filename in ("state.safetensors", "parameter-order.json"):
                shutil.copy2(source / filename, shadow / filename)
            outer_config = read(source / "config.json")
            require(outer_config["search"]["horizon"] == 6,
                    f"source model contract changed: {stream}")
            outer_config["search"] = read(config)
            write(shadow / "config.json", outer_config)
            configurations[key] = sha(shadow / "config.json")
    lower, _ = initial_models()
    write(mixed / "protocol.json", {
        "schema": "aic-hierarchical-horizon-mixed-seed-protocol-v1",
        "runner_sha256": sha(Path(__file__)),
        "cross_protocol_sha256": sha(root / "cross/protocol.json"),
        "cross_progress_sha256": sha(root / "cross/progress.json"),
        "cross_summary_sha256": sha(root / "cross/summary.json"),
        "stress_protocol_sha256": sha(root / "stress/protocol.json"),
        "stress_progress_sha256": sha(root / "stress/progress.json"),
        "stress_summary_sha256": sha(root / "stress/summary.json"),
        "binary_sha256": sha(BINARY),
        "lower_sha256": sha(lower / "state.safetensors"),
        "panel_sha256": sha(root / "panels/onpolicy-diagnostic.json"),
        "new_seeds": list(NEW_SEEDS),
        "model_sha256": models,
        "parameter_order_sha256": orders,
        "shadow_config_sha256": configurations,
        "gate": {
            "mean_gain_vs_h6": 4,
            "positive_seed_means_vs_h6": 2,
            "mean_fewer_lost_vs_h6": 3,
            "minimum_mean_difference_vs_h12": -2,
            "minimum_each_seed_mean_vs_h12": -2,
            "minimum_mean_fewer_lost_vs_h12": 0,
        },
        "planned_records": len(NEW_SEEDS) * len(STREAMS),
    })
    write(mixed / "progress.json", {
        "schema": "aic-hierarchical-horizon-mixed-seed-progress-v1",
        "protocol_sha256": sha(mixed / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 streams x 2 new root seeds")


def validate(root):
    cross_protocol, cross_state, _, _ = validate_cross(root)
    mixed = root / "mixed-seed"
    protocol = read(mixed / "protocol.json")
    state = read(mixed / "progress.json")
    lower, _ = initial_models()
    require(cross_state["status"] == "complete"
            and state["protocol_sha256"] == sha(mixed / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["cross_protocol_sha256"] == sha(root / "cross/protocol.json")
            and protocol["cross_progress_sha256"] == sha(root / "cross/progress.json")
            and protocol["cross_summary_sha256"] == sha(root / "cross/summary.json")
            and protocol["stress_protocol_sha256"] == sha(root / "stress/protocol.json")
            and protocol["stress_progress_sha256"] == sha(root / "stress/progress.json")
            and protocol["stress_summary_sha256"] == sha(root / "stress/summary.json")
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["lower_sha256"] == sha(lower / "state.safetensors")
            and protocol["panel_sha256"] == sha(root / "panels/onpolicy-diagnostic.json")
            and protocol["new_seeds"] == list(NEW_SEEDS)
            and protocol["planned_records"] == len(NEW_SEEDS) * len(STREAMS),
            "mixed-seed input changed")
    for stream in STREAMS:
        source = source_model(root, stream, 6)
        require(protocol["model_sha256"][stream] == sha(source / "state.safetensors"),
                f"model changed: {stream}")
        require(protocol["parameter_order_sha256"][stream]
                == sha(source / "parameter-order.json"),
                f"parameter order changed: {stream}")
        for seed in NEW_SEEDS:
            key = name(stream, seed)
            config = mixed / "configs" / f"search-h12-{seed}.json"
            shadow = mixed / "models" / key
            require(read(config) == seeded_search(root, HORIZON, seed)
                    and sha(shadow / "state.safetensors")
                    == protocol["model_sha256"][stream]
                    and sha(shadow / "parameter-order.json")
                    == protocol["parameter_order_sha256"][stream]
                    and protocol["shadow_config_sha256"][key]
                    == sha(shadow / "config.json")
                    and read(shadow / "config.json")["search"] == read(config),
                    f"seeded model contract changed: {key}")
    return protocol, state


def run(root, max_records):
    protocol, state = validate(root)
    mixed = root / "mixed-seed"
    lower, _ = initial_models()
    panel = root / "panels/onpolicy-diagnostic.json"
    cases = read(panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    added = 0
    state["status"] = "running"
    write(mixed / "progress.json", state)
    for seed in NEW_SEEDS:
        for stream in STREAMS:
            key = name(stream, seed)
            if any(row["name"] == key for row in state["records"]):
                continue
            if max_records is not None and added >= max_records:
                return
            config = mixed / "configs" / f"search-h12-{seed}.json"
            model = mixed / "models" / key
            output = mixed / "evaluations" / f"{key}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            seconds = None
            if not output.exists():
                seconds = cli(mixed, key, [
                    "hierarchical-search-harness", "--config", config,
                    "--panel", panel, "--lower-training-output", lower,
                    "--outer-training-output", model,
                    "--catalog-root", "data/game/normalized",
                    "--output", output, "--device", "cuda:0"])
            report = read(output)
            require(report["status"] == "complete"
                    and report["panel_sha256"] == protocol["panel_sha256"]
                    and report["search_config_sha256"]
                    == semantic_hash(read(config))
                    and report["lower_checkpoint_sha256"] == protocol["lower_sha256"]
                    and report["outer_checkpoint_sha256"]
                    == protocol["model_sha256"][stream],
                    f"mixed search lineage differs: {key}")
            result = {k: v for k, v in outcomes(report, order, easy).items()
                      if k in ("case_success", "completed")}
            state["records"].append(dict(result, name=key, stream=stream,
                                         seed=seed, report_sha256=sha(output),
                                         evaluation_seconds=seconds))
            write(mixed / "progress.json", state)
            added += 1
            print(f"mixed={key} completed={result['completed']}/64", flush=True)
    require(len(state["records"]) == protocol["planned_records"],
            "mixed-seed search incomplete")
    state["status"] = "complete"
    write(mixed / "progress.json", state)


def pair(candidate, control):
    a = candidate["case_success"]
    b = control["case_success"]
    require(len(a) == len(b) == 64, "paired case masks differ")
    return {"completion_difference": candidate["completed"] - control["completed"],
            "gained": sum(x == "1" and y == "0" for x, y in zip(a, b)),
            "lost": sum(x == "0" and y == "1" for x, y in zip(a, b))}


def lost_start(record, starting):
    a = record["case_success"]
    b = starting["case_success"]
    require(len(a) == len(b) == 64, "starting case masks differ")
    return sum(x == "0" and y == "1" for x, y in zip(a, b))


def summarize(root):
    protocol, state = validate(root)
    require(state["status"] == "complete"
            and len(state["records"]) == protocol["planned_records"],
            "mixed-seed search incomplete")
    stress = read(root / "stress/progress.json")
    old = {r["name"]: r for r in stress["records"]}
    mixed = {(r["seed"], r["stream"]): r for r in state["records"]}
    rows = []
    for seed in NEW_SEEDS:
        start = old[f"start-h6-{seed}"]
        for stream in STREAMS:
            candidate = mixed[(seed, stream)]
            h6 = old[f"{stream}-h6-{seed}"]
            h12 = old[f"{stream}-h12-{seed}"]
            rows.append({
                "seed": seed, "stream": stream,
                "mixed_completed": candidate["completed"],
                "h6_diagonal_completed": h6["completed"],
                "h12_diagonal_completed": h12["completed"],
                "mixed_vs_h6": pair(candidate, h6),
                "mixed_vs_h12": pair(candidate, h12),
                "mixed_lost_start": lost_start(candidate, start),
                "h6_lost_start": lost_start(h6, start),
                "h12_lost_start": lost_start(h12, start),
            })
    gate = protocol["gate"]
    vs_h6 = [r["mixed_vs_h6"]["completion_difference"] for r in rows]
    vs_h12 = [r["mixed_vs_h12"]["completion_difference"] for r in rows]
    fewer_h6 = [r["h6_lost_start"] - r["mixed_lost_start"] for r in rows]
    fewer_h12 = [r["h12_lost_start"] - r["mixed_lost_start"] for r in rows]
    seed_means_h6 = {str(seed): statistics.mean(r["mixed_vs_h6"][
        "completion_difference"] for r in rows if r["seed"] == seed)
        for seed in NEW_SEEDS}
    seed_means_h12 = {str(seed): statistics.mean(r["mixed_vs_h12"][
        "completion_difference"] for r in rows if r["seed"] == seed)
        for seed in NEW_SEEDS}
    checks = {
        "mean_gain_vs_h6": statistics.mean(vs_h6) >= gate["mean_gain_vs_h6"],
        "positive_seed_means_vs_h6": sum(v > 0 for v in seed_means_h6.values())
                                     >= gate["positive_seed_means_vs_h6"],
        "mean_fewer_lost_vs_h6": statistics.mean(fewer_h6)
                                 >= gate["mean_fewer_lost_vs_h6"],
        "minimum_mean_difference_vs_h12": statistics.mean(vs_h12)
                                           >= gate["minimum_mean_difference_vs_h12"],
        "minimum_each_seed_mean_vs_h12": all(
            v >= gate["minimum_each_seed_mean_vs_h12"]
            for v in seed_means_h12.values()),
        "minimum_mean_fewer_lost_vs_h12": statistics.mean(fewer_h12)
                                          >= gate["minimum_mean_fewer_lost_vs_h12"],
    }
    result = {
        "schema": "aic-hierarchical-horizon-mixed-seed-summary-v1",
        "protocol_sha256": sha(root / "mixed-seed/protocol.json"),
        "progress_sha256": sha(root / "mixed-seed/progress.json"),
        "rows": rows,
        "mean_completion_gain_vs_h6": statistics.mean(vs_h6),
        "seed_mean_completion_gain_vs_h6": seed_means_h6,
        "mean_fewer_lost_start_vs_h6": statistics.mean(fewer_h6),
        "mean_completion_gain_vs_h12": statistics.mean(vs_h12),
        "seed_mean_completion_gain_vs_h12": seed_means_h12,
        "mean_fewer_lost_start_vs_h12": statistics.mean(fewer_h12),
        "checks": checks,
        "passes_mixed_selection_gate": all(checks.values()),
    }
    write(root / "mixed-seed/summary.json", result)
    print(f"mixed_vs_h6={result['mean_completion_gain_vs_h6']} "
          f"mixed_vs_h12={result['mean_completion_gain_vs_h12']} "
          f"passes={result['passes_mixed_selection_gate']}")


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
