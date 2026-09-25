"""Evaluate a passing restored-lower setup on the reserved held-out panel."""

import argparse
from pathlib import Path
import shutil
import statistics

from hierarchical_lower_restored import (checkpoint, search, search_config,
                                         validate as validate_parent)
from hierarchical_horizon_onpolicy import require
from hierarchical_success_onpolicy import SEEDS
from hierarchical_target_learner import BINARY, STREAMS, outcomes
from ppo_harness_state import read, sha, write


ARMS = ("h6", "h12")
GATE = {
    "mean_completion_gain": 8,
    "positive_seed_means": 2,
    "mean_fewer_lost_start_successes": 6,
    "positive_seed_retention_means": 2,
}


def model(parent, stream, arm):
    return checkpoint(parent, stream, 2, arm) if stream != "start" else checkpoint(
        parent, STREAMS[0], -1, "h6")


def configurations(parent, seed):
    result = {}
    for arm in ARMS:
        config = read(search_config(parent, arm))
        require(config["root_selection"]["seed"] == SEEDS[0],
                "source search seed changed")
        config["root_selection"]["seed"] = seed
        result[arm] = config
    return result


def key(seed, stream, arm):
    return f"{stream}-{arm}-{seed}"


def prepare(parent):
    parent_protocol, progress = validate_parent(parent)
    primary = read(parent / "primary-summary.json")
    observer = read(parent / "wandb-observer.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == parent_protocol["planned_records"]
            and primary["progress_sha256"] == sha(parent / "progress.json")
            and primary["passes_primary_gate"]
            and primary["selected_candidate"] == parent_protocol["selected_candidate"] == "h12"
            and observer["status"] == "finished"
            and observer["exported_records"] == parent_protocol["planned_records"],
            "primary gate or tracking closeout incomplete")
    heldout = parent / "heldout-check"
    require(not heldout.exists(), "held-out check already exists")
    source_panel = parent / "panels/heldout.json"
    cases = read(source_panel)["cases"]
    require(len(cases) == 128 and read(source_panel)["partition"] == "held-out",
            "reserved held-out panel changed")
    heldout.mkdir()
    configs = {}
    for seed in SEEDS:
        for arm, config in configurations(parent, seed).items():
            path = heldout / "configs" / f"search-{arm}-{seed}.json"
            path.parent.mkdir(parents=True, exist_ok=True)
            write(path, config)
            configs[f"{arm}-{seed}"] = sha(path)
    models = {}
    shadows = {}
    for stream in ("start", *STREAMS):
        for arm in (("h6",) if stream == "start" else ARMS):
            lower, outer = model(parent, stream, arm)
            label = f"{stream}-{arm}"
            if stream != "start":
                row = next(row for row in progress["records"]
                           if row["name"] == f"{stream}-g2-{arm}")
                require(row["lower_checkpoint_sha256"] == sha(lower / "state.safetensors")
                        and row["outer_checkpoint_sha256"] == sha(outer / "state.safetensors"),
                        f"final model lineage changed: {label}")
            models[label] = {
                "lower": sha(lower / "state.safetensors"),
                "lower_order": sha(lower / "parameter-order.json"),
                "outer": sha(outer / "state.safetensors"),
                "outer_order": sha(outer / "parameter-order.json"),
            }
            for seed in SEEDS:
                destination = heldout / "models" / key(seed, stream, arm)
                destination.mkdir(parents=True)
                for filename in ("state.safetensors", "parameter-order.json"):
                    shutil.copy2(outer / filename, destination / filename)
                config = read(outer / "config.json")
                require(config["search"]["horizon"] == (6 if arm == "h6" else 12),
                        f"model search contract changed: {label}")
                config["search"] = configurations(parent, seed)[arm]
                write(destination / "config.json", config)
                shadows[key(seed, stream, arm)] = sha(destination / "config.json")
    write(heldout / "protocol.json", {
        "schema": "aic-hierarchical-lower-restored-heldout-protocol-v1",
        "runner_sha256": sha(Path(__file__)),
        "parent_runner_sha256": parent_protocol["runner_sha256"],
        "parent_protocol_sha256": sha(parent / "protocol.json"),
        "parent_progress_sha256": sha(parent / "progress.json"),
        "primary_summary_sha256": sha(parent / "primary-summary.json"),
        "binary_sha256": sha(BINARY),
        "panel_sha256": sha(source_panel),
        "config_sha256": configs,
        "model_sha256": models,
        "shadow_config_sha256": shadows,
        "selected_candidate": "h12",
        "seeds": list(SEEDS),
        "gate": GATE,
        "planned_records": len(SEEDS) * (1 + len(STREAMS) * len(ARMS)),
    })
    write(heldout / "progress.json", {
        "schema": "aic-hierarchical-lower-restored-heldout-progress-v1",
        "protocol_sha256": sha(heldout / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 seeds x (starting H6 + 3 H6 finals + 3 H12 finals)")


def validate(parent):
    parent_protocol, parent_progress = validate_parent(parent)
    heldout = parent / "heldout-check"
    protocol = read(heldout / "protocol.json")
    progress = read(heldout / "progress.json")
    require(parent_progress["status"] == "complete"
            and progress["protocol_sha256"] == sha(heldout / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["parent_runner_sha256"] == parent_protocol["runner_sha256"]
            and protocol["parent_protocol_sha256"] == sha(parent / "protocol.json")
            and protocol["parent_progress_sha256"] == sha(parent / "progress.json")
            and protocol["primary_summary_sha256"] == sha(parent / "primary-summary.json")
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["panel_sha256"] == sha(parent / "panels/heldout.json")
            and protocol["selected_candidate"] == "h12"
            and protocol["seeds"] == list(SEEDS)
            and protocol["gate"] == GATE
            and protocol["planned_records"] == len(SEEDS) * 7,
            "held-out input changed")
    for seed in SEEDS:
        for arm, source_config in configurations(parent, seed).items():
            path = heldout / "configs" / f"search-{arm}-{seed}.json"
            require(read(path) == source_config
                    and protocol["config_sha256"][f"{arm}-{seed}"] == sha(path),
                    f"seeded search changed: {arm}-{seed}")
    for stream in ("start", *STREAMS):
        for arm in (("h6",) if stream == "start" else ARMS):
            lower, outer = model(parent, stream, arm)
            hashes = protocol["model_sha256"][f"{stream}-{arm}"]
            require(hashes == {
                "lower": sha(lower / "state.safetensors"),
                "lower_order": sha(lower / "parameter-order.json"),
                "outer": sha(outer / "state.safetensors"),
                "outer_order": sha(outer / "parameter-order.json"),
            }, f"model changed: {stream}-{arm}")
            for seed in SEEDS:
                name = key(seed, stream, arm)
                shadow = heldout / "models" / name
                require(sha(shadow / "state.safetensors") == hashes["outer"]
                        and sha(shadow / "parameter-order.json") == hashes["outer_order"]
                        and protocol["shadow_config_sha256"][name]
                        == sha(shadow / "config.json")
                        and read(shadow / "config.json")["search"]
                        == read(heldout / "configs" / f"search-{arm}-{seed}.json"),
                        f"seeded model changed: {name}")
    return protocol, progress


def run(parent, max_records):
    protocol, progress = validate(parent)
    heldout = parent / "heldout-check"
    panel = parent / "panels/heldout.json"
    cases = read(panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    progress["status"] = "running"
    write(heldout / "progress.json", progress)
    added = 0
    for seed in SEEDS:
        for stream in ("start", *STREAMS):
            for arm in (("h6",) if stream == "start" else ARMS):
                name = key(seed, stream, arm)
                if any(row["name"] == name for row in progress["records"]):
                    continue
                if max_records is not None and added >= max_records:
                    return
                lower, _ = model(parent, stream, arm)
                shadow = heldout / "models" / name
                config = heldout / "configs" / f"search-{arm}-{seed}.json"
                output = heldout / "evaluations" / f"{name}.json"
                report, seconds = search(heldout, name, panel, lower,
                                         shadow, config, output)
                result = outcomes(report, order, easy)
                progress["records"].append(dict(result, name=name, seed=seed,
                                                stream=stream, arm=arm,
                                                report_sha256=sha(output),
                                                evaluation_seconds=seconds))
                write(heldout / "progress.json", progress)
                added += 1
                print(f"heldout={name} completed={result['completed']}/128", flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "held-out evaluation incomplete")
    progress["status"] = "complete"
    write(heldout / "progress.json", progress)


def losses(candidate, baseline):
    require(len(candidate) == len(baseline) == 128, "held-out masks differ")
    return sum(a == "0" and b == "1" for a, b in zip(candidate, baseline))


def paired_quality(parent, seed, stream):
    heldout = parent / "heldout-check"
    control = read(heldout / "evaluations" / f"{key(seed, stream, 'h6')}.json")
    candidate = read(heldout / "evaluations" / f"{key(seed, stream, 'h12')}.json")
    a = {case["id"]: case["mcts"] for case in control["cases"]}
    b = {case["id"]: case["mcts"] for case in candidate["cases"]}
    require(set(a) == set(b), "paired quality cases differ")
    common = [case_id for case_id in a if a[case_id]["fully_validated"]
              and b[case_id]["fully_validated"]]
    result = {"both_successful": len(common)}
    for metric in ("footprint_area", "transport_cells"):
        require(all(a[case_id][metric] is not None
                    and b[case_id][metric] is not None for case_id in common),
                f"missing validated {metric}")
        differences = [b[case_id][metric] - a[case_id][metric] for case_id in common]
        result[f"candidate_minus_control_{metric}_mean"] = (
            statistics.mean(differences) if differences else None)
    return result


def summarize(parent):
    protocol, progress = validate(parent)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "held-out evaluation incomplete")
    heldout = parent / "heldout-check"
    rows = {(row["seed"], row["stream"], row["arm"]): row
            for row in progress["records"]}
    comparisons = []
    for seed in SEEDS:
        baseline = rows[(seed, "start", "h6")]
        for stream in STREAMS:
            control = rows[(seed, stream, "h6")]
            candidate = rows[(seed, stream, "h12")]
            for row in (baseline, control, candidate):
                output = heldout / "evaluations" / f"{row['name']}.json"
                require(row["report_sha256"] == sha(output),
                        f"held-out report changed: {row['name']}")
            mask_a = candidate["case_success"]
            mask_b = control["case_success"]
            require(len(mask_a) == len(mask_b) == 128,
                    "paired held-out masks differ")
            comparisons.append({
                "seed": seed, "stream": stream,
                "control_completed": control["completed"],
                "candidate_completed": candidate["completed"],
                "completion_gain": candidate["completed"] - control["completed"],
                "candidate_gained_cases": sum(a == "1" and b == "0"
                                              for a, b in zip(mask_a, mask_b)),
                "candidate_lost_cases": sum(a == "0" and b == "1"
                                            for a, b in zip(mask_a, mask_b)),
                "control_lost_start": losses(mask_b, baseline["case_success"]),
                "candidate_lost_start": losses(mask_a, baseline["case_success"]),
                "conditional_quality": paired_quality(parent, seed, stream),
            })
    gains = [row["completion_gain"] for row in comparisons]
    retention = [row["control_lost_start"] - row["candidate_lost_start"]
                 for row in comparisons]
    seed_gains = {str(seed): statistics.mean(
        row["completion_gain"] for row in comparisons if row["seed"] == seed)
        for seed in SEEDS}
    seed_retention = {str(seed): statistics.mean(
        row["control_lost_start"] - row["candidate_lost_start"]
        for row in comparisons if row["seed"] == seed) for seed in SEEDS}
    gate = protocol["gate"]
    checks = {
        "mean_completion_gain": statistics.mean(gains) >= gate["mean_completion_gain"],
        "positive_seed_means": sum(value > 0 for value in seed_gains.values())
                               >= gate["positive_seed_means"],
        "mean_fewer_lost_start_successes": statistics.mean(retention)
                                           >= gate["mean_fewer_lost_start_successes"],
        "positive_seed_retention_means": sum(value > 0 for value in seed_retention.values())
                                         >= gate["positive_seed_retention_means"],
    }
    result = {
        "schema": "aic-hierarchical-lower-restored-heldout-summary-v1",
        "protocol_sha256": sha(heldout / "protocol.json"),
        "progress_sha256": sha(heldout / "progress.json"),
        "mean_completion_gain": statistics.mean(gains),
        "seed_mean_completion_gain": seed_gains,
        "mean_fewer_lost_start_successes": statistics.mean(retention),
        "seed_mean_fewer_lost_start_successes": seed_retention,
        "comparisons": comparisons,
        "checks": checks,
        "passes_heldout_gate": all(checks.values()),
    }
    write(heldout / "summary.json", result)
    print(f"heldout_mean_gain={result['mean_completion_gain']} "
          f"passes_heldout_gate={result['passes_heldout_gate']}")


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
