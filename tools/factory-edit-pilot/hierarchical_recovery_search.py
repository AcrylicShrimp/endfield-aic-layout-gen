"""Run the frozen-model search mechanism screen on retained audit cases."""

import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time

from ppo_harness_state import read, sha, write


STREAMS = ("s1", "s2", "s3")
SEEDS = (20260922, 20260923, 20260924)
ARMS = ("base", "q02", "q0", "anchor16", "anchor0", "horizon12")
BASE = Path("docs/benchmarks/hierarchical-target-causal")
PREVIOUS = Path("docs/reports/2026-09-25-hierarchical-target-quality-protocol.json")
BINARY = Path("target/release/aic-cli")


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-recovery-search: {message}")


def action_digest(action):
    encoded = json.dumps(action["action"], sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(encoded.encode()).hexdigest()


def name(arm, seed):
    return f"{arm}-{seed}"


def paths(root, arm, seed):
    label = name(arm, seed)
    return (root / "configs" / f"{label}-search.json",
            root / "configs" / f"{label}-outer.json",
            root / "models" / label)


def make_config(search, arm, seed):
    config = json.loads(json.dumps(search))
    config["root_selection"]["seed"] = seed
    if arm == "q02":
        config["root_selection"]["value_scale"] = 0.02
    elif arm == "q0":
        config["root_selection"]["value_scale"] = 0.0
    elif arm == "anchor16":
        config["root_selection"]["anchor_candidates"] = 16
    elif arm == "anchor0":
        config["root_selection"]["anchor_candidates"] = 0
    elif arm == "horizon12":
        config["horizon"] = 12
    else:
        require(arm == "base", "unknown arm")
    return config


def prepare(root, commit):
    require(root.is_dir() and not (root / "protocol.json").exists(),
            "prepare the prior-audit panels in a fresh root first")
    previous = read(PREVIOUS)
    for stream in STREAMS:
        require(sha(root / "inputs/panels" / f"{stream}.json")
                == previous["panel_sha256"][stream], f"panel differs: {stream}")
    lower = root / "inputs/models/lower/state.safetensors"
    outer = root / "inputs/models/outer-128/state.safetensors"
    require(sha(lower) == previous["lower_checkpoint_sha256"], "lower changed")
    require(sha(outer) == previous["outer_checkpoint_sha256"], "outer changed")
    base_search = read(BASE / "c16-n128-search.json")
    base_outer = read(BASE / "c16-n128-outer.json")
    require(base_outer["search"] == base_search
            and base_search["root_selection"]["seed"] == SEEDS[0]
            and base_search["root_selection"]["candidates"] == 16
            and base_search["simulations_per_move"] == 128,
            "accepted baseline contract differs")
    (root / "configs").mkdir(parents=True, exist_ok=True)
    configs = {}
    for arm in ARMS:
        for seed in SEEDS:
            search_path, outer_path, model = paths(root, arm, seed)
            search = make_config(base_search, arm, seed)
            write(search_path, search)
            write(outer_path, dict(base_outer, search=search))
            model.mkdir(parents=True, exist_ok=True)
            for filename in ("state.safetensors", "parameter-order.json"):
                (model / filename).symlink_to(
                    Path("../../inputs/models/outer-128") / filename)
            write(model / "config.json", dict(base_outer, search=search))
            configs[name(arm, seed)] = {
                "search": sha(search_path), "outer": sha(outer_path),
                "model_config": sha(model / "config.json"),
            }
    protocol = {
        "schema": "aic-hierarchical-recovery-search-protocol-v1",
        "experiment_commit": commit,
        "runner_sha256": sha(Path(__file__)),
        "binary_sha256": sha(BINARY),
        "lower_sha256": sha(lower), "outer_sha256": sha(outer),
        "panels": {s: sha(root / "inputs/panels" / f"{s}.json") for s in STREAMS},
        "configs": configs, "arms": ARMS, "seeds": SEEDS,
        "planned_cells": len(STREAMS) * len(ARMS) * len(SEEDS),
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {
        "schema": "aic-hierarchical-recovery-search-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print(f"prepared_cells={protocol['planned_cells']}")


def validate(root):
    protocol, progress = read(root / "protocol.json"), read(root / "progress.json")
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["binary_sha256"] == sha(BINARY),
            "protocol, code, or binary changed")
    for stream, expected in protocol["panels"].items():
        require(sha(root / "inputs/panels" / f"{stream}.json") == expected,
                f"panel changed: {stream}")
    require(sha(root / "inputs/models/lower/state.safetensors")
            == protocol["lower_sha256"]
            and sha(root / "inputs/models/outer-128/state.safetensors")
            == protocol["outer_sha256"], "model changed")
    for arm in ARMS:
        for seed in SEEDS:
            config, outer, model = paths(root, arm, seed)
            expected = protocol["configs"][name(arm, seed)]
            require(sha(config) == expected["search"]
                    and sha(outer) == expected["outer"]
                    and sha(model / "config.json") == expected["model_config"]
                    and sha(model / "state.safetensors") == protocol["outer_sha256"],
                    f"configuration changed: {arm}/{seed}")
    return protocol, progress


def run(root, max_cells):
    protocol, progress = validate(root)
    progress["status"] = "running"
    write(root / "progress.json", progress)
    count = 0
    for stream in STREAMS:
        for seed in SEEDS:
            for arm in ARMS:
                key = (stream, seed, arm)
                if any((row["stream"], row["seed"], row["arm"]) == key
                       for row in progress["records"]):
                    continue
                if max_cells is not None and count >= max_cells:
                    return
                config, _, model = paths(root, arm, seed)
                output = root / "raw" / stream / f"{name(arm, seed)}.json"
                output.parent.mkdir(parents=True, exist_ok=True)
                log = root / "logs" / f"{stream}-{name(arm, seed)}.log"
                log.parent.mkdir(parents=True, exist_ok=True)
                seconds = None
                if not output.exists():
                    command = [str(BINARY), "learning", "hierarchical-search-harness",
                               "--config", str(config), "--panel",
                               str(root / "inputs/panels" / f"{stream}.json"),
                               "--lower-training-output", str(root / "inputs/models/lower"),
                               "--outer-training-output", str(model),
                               "--catalog-root", "data/game/normalized",
                               "--output", str(output), "--device", "cuda:0"]
                    started = time.monotonic()
                    with log.open("w") as handle:
                        result = subprocess.run(command, stdout=handle,
                                                stderr=subprocess.STDOUT, check=False)
                    require(result.returncode == 0,
                            f"search failed: {stream}/{seed}/{arm}, see {log}")
                    seconds = time.monotonic() - started
                report = read(output)
                require(report["status"] == "complete"
                        and report["panel_sha256"] == protocol["panels"][stream]
                        and bool(report["search_config_sha256"])
                        and report["lower_checkpoint_sha256"] == protocol["lower_sha256"]
                        and report["outer_checkpoint_sha256"] == protocol["outer_sha256"]
                        and len(report["cases"]) == 16,
                        f"bad report: {stream}/{seed}/{arm}")
                progress["records"].append({
                    "stream": stream, "seed": seed, "arm": arm,
                    "report_sha256": sha(output), "report_bytes": output.stat().st_size,
                    "wall_seconds": seconds,
                })
                write(root / "progress.json", progress)
                count += 1
                completed = sum(bool(row["mcts"]["fully_validated"])
                                for row in report["cases"])
                print(f"cell={stream}/{seed}/{arm} completed={completed}/16 "
                      f"seconds={seconds}", flush=True)
    require(len(progress["records"]) == protocol["planned_cells"], "incomplete")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def summarize(root):
    protocol, progress = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"],
            "search screen incomplete")
    cases = {}
    for stream in STREAMS:
        for seed in SEEDS:
            base = None
            for arm in ARMS:
                record = next(row for row in progress["records"]
                              if (row["stream"], row["seed"], row["arm"])
                              == (stream, seed, arm))
                report = read(root / "raw" / stream / f"{name(arm, seed)}.json")
                require(sha(root / "raw" / stream / f"{name(arm, seed)}.json")
                        == record["report_sha256"], "report changed")
                if arm == "base":
                    base = report
                for index, row in enumerate(report["cases"]):
                    case_id = row["id"]
                    witness = row["mcts"]["root_records"][0]
                    reference = base["cases"][index]["mcts"]["root_records"][0]
                    require(case_id == base["cases"][index]["id"]
                            and witness["state_sha256"] == reference["state_sha256"]
                            and [action_digest(a) for a in witness["actions"]]
                            == [action_digest(a) for a in reference["actions"]],
                            f"first-root witness differs: {stream}/{seed}/{arm}/{index}")
                    selected = action_digest(witness["actions"][witness["selected_action"]])
                    item = cases.setdefault(case_id, {"stream": stream, "case_id": case_id,
                                                      "arms": {}})
                    item["arms"].setdefault(arm, {})[str(seed)] = {
                        "success": bool(row["mcts"]["fully_validated"]),
                        "completed_connections": row["mcts"]["completed_connections"],
                        "total_connections": row["mcts"]["total_connections"],
                        "termination": row["mcts"]["termination"],
                        "first_action_sha256": selected,
                        "elapsed_seconds": row["mcts"]["elapsed_seconds"],
                    }
    require(len(cases) == 48, "case count differs")
    rows = list(cases.values())
    summaries = {}
    for arm in ARMS:
        per_seed = [sum(row["arms"][arm][str(seed)]["success"] for row in rows)
                    for seed in SEEDS]
        baseline = [sum(row["arms"]["base"][str(seed)]["success"] for row in rows)
                    for seed in SEEDS]
        robust = sum(sum(value["success"] for value in row["arms"][arm].values()) >= 2
                     for row in rows)
        base_robust = sum(sum(value["success"] for value in row["arms"]["base"].values()) >= 2
                          for row in rows)
        lost_robust = sum(
            sum(v["success"] for v in row["arms"]["base"].values()) >= 2
            and sum(v["success"] for v in row["arms"][arm].values()) < 2
            for row in rows)
        paired = [{"gain": sum(row["arms"][arm][str(seed)]["success"]
                               and not row["arms"]["base"][str(seed)]["success"]
                               for row in rows),
                   "loss": sum(row["arms"]["base"][str(seed)]["success"]
                               and not row["arms"][arm][str(seed)]["success"]
                               for row in rows)} for seed in SEEDS]
        summaries[arm] = {
            "completed_per_seed": per_seed,
            "mean_completed": statistics.mean(per_seed),
            "mean_gain_over_base": statistics.mean(per_seed) - statistics.mean(baseline),
            "robust_successes": robust,
            "robust_lost_from_base": lost_robust,
            "paired_per_seed": paired,
            "first_actions_equal_base_per_seed": [sum(
                row["arms"][arm][str(seed)]["first_action_sha256"]
                == row["arms"]["base"][str(seed)]["first_action_sha256"]
                for row in rows) for seed in SEEDS],
            "wall_seconds": sum(record["wall_seconds"] or 0
                                for record in progress["records"] if record["arm"] == arm),
            "passes_screen": (arm != "base"
                              and statistics.mean(per_seed) - statistics.mean(baseline) >= 3
                              and sum(a > b for a, b in zip(per_seed, baseline)) >= 2
                              and lost_robust <= 2),
        }
        require(base_robust == summaries["base"]["robust_successes"]
                if "base" in summaries else True, "baseline changed")
    result = {"schema": "aic-hierarchical-recovery-search-summary-v1",
              "protocol_sha256": sha(root / "protocol.json"),
              "progress_sha256": sha(root / "progress.json"),
              "cases": rows, "summary": summaries}
    write(root / "summary.json", result)
    for arm, row in summaries.items():
        print(f"{arm}: {row['completed_per_seed']} mean_delta="
              f"{row['mean_gain_over_base']:.2f} robust={row['robust_successes']} "
              f"lost_robust={row['robust_lost_from_base']} "
              f"gate={row['passes_screen']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-cells", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    elif args.command == "run":
        run(args.output, args.max_cells)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
