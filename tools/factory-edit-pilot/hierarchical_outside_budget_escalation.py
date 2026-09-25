"""Escalate nonreplicated outside-candidate first-action pairs to 512 later simulations."""

import argparse
import json
from pathlib import Path
import subprocess
import time

from hierarchical_continuation_budget_control import SEEDS, config
from ppo_harness_state import read, sha, write


def require(ok, message):
    if not ok:
        raise ValueError(f"outside-budget-escalation: {message}")


def prepare(root, commit):
    require(not (root / "outside-budget-protocol.json").exists(), "already prepared")
    primary = read(root / "continuation-summary.json")
    replication = read(root / "replication-summary.json")
    by_id = {row["case_id"]: row for row in replication["cases"]}
    pairs = []
    for case in primary["cases"]:
        repeated = by_id.get(case["case_id"])
        if repeated is None:
            continue
        baseline = case["variants"][0]["completion_count"]
        repeated_baseline = repeated["replication_completion_counts"][0]
        for variant in case["variants"][1:]:
            rank = variant["rank"]
            if (variant["baseline_visits"] == 0
                    and variant["completion_count"] - baseline >= 2
                    and repeated["replication_completion_counts"][rank]
                    - repeated_baseline < 2):
                pairs.append({"case_id": case["case_id"], "stream": case["stream"],
                              "rank": rank, "action_sha256": variant["action_sha256"],
                              "primary_counts": [baseline, variant["completion_count"]],
                              "replication_counts": [repeated_baseline,
                                  repeated["replication_completion_counts"][rank]],
                              "first_state_sha256": variant["trials"][0]["first_state_sha256"],
                              "baseline_state_sha256": case["variants"][0]["trials"][0]["first_state_sha256"]})
    require(len(pairs) == 10 and len({p["case_id"] for p in pairs}) == 9,
            f"selection differs: {len(pairs)} pairs")
    ranks = sorted({0, *(p["rank"] for p in pairs)})
    panels = {}
    for rank in ranks:
        selected = {p["case_id"] for p in pairs if rank in (0, p["rank"])}
        source = read(root / f"inputs/continuation-panels/rank-{rank}.json")
        source["cases"] = [case for case in source["cases"] if case["id"] in selected]
        require(len(source["cases"]) == len(selected), f"missing panel cases: {rank}")
        path = root / f"inputs/outside-budget-panels/rank-{rank}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        write(path, source)
        panels[str(rank)] = {"sha256": sha(path), "case_ids": sorted(selected)}
    hashes = {}
    for seed in SEEDS:
        search, outer = read(config(seed, "search")), read(config(seed, "outer"))
        require(search["max_moves"] == 23 and search["simulations_per_move"] == 512
                and search["root_selection"]["candidates"] == 16
                and search["root_selection"]["seed"] == seed
                and outer["search"] == search, f"config differs: {seed}")
        model = root / f"inputs/models/outer-budget-512-{seed}"
        require(sha(model / "config.json") == sha(config(seed, "outer")),
                f"model config differs: {seed}")
        hashes[str(seed)] = {"search": sha(config(seed, "search")),
                             "outer": sha(config(seed, "outer"))}
    protocol = {"schema": "aic-hierarchical-outside-budget-protocol-v1",
                "experiment_commit": commit, "pairs": pairs, "panels": panels,
                "primary_sha256": sha(root / "continuation-summary.json"),
                "replication_sha256": sha(root / "replication-summary.json"),
                "binary_sha256": sha(Path("target/release/aic-cli")),
                "runner_sha256": sha(Path(__file__)),
                "lower_sha256": sha(root / "inputs/models/lower/state.safetensors"),
                "outer_sha256": sha(root / "inputs/models/outer-128/state.safetensors"),
                "configs": hashes, "planned_cells": len(SEEDS)*len(panels)}
    write(root / "outside-budget-protocol.json", protocol)
    write(root / "outside-budget-progress.json", {"schema": "aic-hierarchical-outside-budget-progress-v1",
        "protocol_sha256": sha(root / "outside-budget-protocol.json"),
        "status": "prepared", "records": []})
    print(f"pairs={len(pairs)} cases=9 cells={protocol['planned_cells']}")


def run(root, max_cells):
    protocol = read(root / "outside-budget-protocol.json")
    progress = read(root / "outside-budget-progress.json")
    require(progress["protocol_sha256"] == sha(root / "outside-budget-protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["binary_sha256"] == sha(Path("target/release/aic-cli"))
            and protocol["primary_sha256"] == sha(root / "continuation-summary.json")
            and protocol["replication_sha256"] == sha(root / "replication-summary.json"),
            "protocol inputs changed")
    progress["status"] = "running"
    write(root / "outside-budget-progress.json", progress)
    completed = 0
    for seed in SEEDS:
        model = root / f"inputs/models/outer-budget-512-{seed}"
        require(sha(config(seed, "search")) == protocol["configs"][str(seed)]["search"]
                and sha(config(seed, "outer")) == protocol["configs"][str(seed)]["outer"]
                and sha(model / "config.json") == protocol["configs"][str(seed)]["outer"]
                and sha(model / "state.safetensors") == protocol["outer_sha256"],
                f"model differs: {seed}")
        for rank, info in protocol["panels"].items():
            if any(row["seed"] == seed and row["rank"] == int(rank)
                   for row in progress["records"]):
                continue
            panel = root / f"inputs/outside-budget-panels/rank-{rank}.json"
            require(sha(panel) == info["sha256"], f"panel differs: {rank}")
            output = root / f"raw/outside-budget/{seed}/rank-{rank}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            log = root / f"logs/outside-budget-{seed}-rank-{rank}.log"
            start = time.monotonic()
            if not output.exists():
                command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                    "--config", str(config(seed, "search")), "--panel", str(panel),
                    "--lower-training-output", str(root / "inputs/models/lower"),
                    "--outer-training-output", str(model), "--catalog-root", "data/game/normalized",
                    "--output", str(output), "--device", "cuda:0"]
                with log.open("w") as stream:
                    result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT,
                                            check=False)
                require(result.returncode == 0, f"search failed: {seed}/{rank}; {log}")
            report = read(output)
            require(report["status"] == "complete" and report["partition"] == "training"
                    and report["panel_sha256"] == info["sha256"]
                    and report["lower_checkpoint_sha256"] == protocol["lower_sha256"]
                    and report["outer_checkpoint_sha256"] == protocol["outer_sha256"]
                    and {case["id"] for case in report["cases"]} == set(info["case_ids"]),
                    f"report differs: {seed}/{rank}")
            progress["records"].append({"seed": seed, "rank": int(rank),
                "report_sha256": sha(output), "report_bytes": output.stat().st_size,
                "wall_seconds": time.monotonic()-start})
            write(root / "outside-budget-progress.json", progress)
            print(f"completed_outside_budget={seed}/{rank}", flush=True)
            completed += 1
            if max_cells is not None and completed >= max_cells:
                return
    require(len(progress["records"]) == protocol["planned_cells"], "incomplete")
    progress["status"] = "complete"
    write(root / "outside-budget-progress.json", progress)


def summarize(root):
    protocol, progress = (read(root / name) for name in
        ("outside-budget-protocol.json", "outside-budget-progress.json"))
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"], "incomplete")
    trials = {}
    for row in progress["records"]:
        path = root / f"raw/outside-budget/{row['seed']}/rank-{row['rank']}.json"
        require(sha(path) == row["report_sha256"], f"raw differs: {path}")
        for case in read(path)["cases"]:
            key = (case["id"], row["rank"])
            first = case["mcts"]["root_records"][0]
            trials.setdefault(key, []).append({"seed": row["seed"],
                "first_state_sha256": first["state_sha256"],
                "completed": case["mcts"]["fully_validated"],
                "completed_connections": case["mcts"]["completed_connections"],
                "total_connections": case["mcts"]["total_connections"]})
    pairs = []
    for pair in protocol["pairs"]:
        baseline = sorted(trials[(pair["case_id"], 0)], key=lambda row: row["seed"])
        candidate = sorted(trials[(pair["case_id"], pair["rank"])],
                           key=lambda row: row["seed"])
        require([row["seed"] for row in baseline] == list(SEEDS)
                and [row["seed"] for row in candidate] == list(SEEDS)
                and all(row["first_state_sha256"] == pair["baseline_state_sha256"]
                        for row in baseline)
                and all(row["first_state_sha256"] == pair["first_state_sha256"]
                        for row in candidate), f"forced first state differs: {pair['case_id']}")
        counts = [sum(row["completed"] for row in baseline),
                  sum(row["completed"] for row in candidate)]
        pairs.append(dict(pair, budget512_counts=counts,
                          budget512_baseline_trials=baseline,
                          budget512_candidate_trials=candidate))
    result = {"schema": "aic-hierarchical-outside-budget-summary-v1",
              "protocol_sha256": sha(root / "outside-budget-protocol.json"),
              "progress_sha256": sha(root / "outside-budget-progress.json"),
              "pairs": pairs}
    write(root / "outside-budget-summary.json", result)
    print({"pairs": len(pairs),
           "candidate_better": sum(p["budget512_counts"][1] > p["budget512_counts"][0]
                                   for p in pairs),
           "baseline_better": sum(p["budget512_counts"][1] < p["budget512_counts"][0]
                                  for p in pairs),
           "tied": sum(p["budget512_counts"][1] == p["budget512_counts"][0]
                       for p in pairs)})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-cells", type=int)
    args = parser.parse_args()
    if args.operation == "prepare":
        require(args.experiment_commit, "experiment commit required")
        prepare(args.output, args.experiment_commit)
    elif args.operation == "run":
        run(args.output, args.max_cells)
    else:
        summarize(args.output)
