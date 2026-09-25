"""Recheck selected outside-action pairs at 512 simulations under new seeds."""

import argparse
import json
from pathlib import Path
import subprocess
import time

from hierarchical_candidate_width import CONFIG_ROOT
from ppo_harness_state import read, sha, write


SEEDS = (20260925, 20260926, 20260927)


def require(ok, message):
    if not ok:
        raise ValueError(f"outside-budget-recheck: {message}")


def config(seed, kind):
    return CONFIG_ROOT / f"outside-budget-recheck-{seed}-{kind}.json"


def write_configs():
    search = read(CONFIG_ROOT / "continuation-budget-512-20260922-search.json")
    outer = read(CONFIG_ROOT / "continuation-budget-512-20260922-outer.json")
    require(outer["search"] == search, "source configs differ")
    for seed in SEEDS:
        trial = json.loads(json.dumps(search))
        trial["root_selection"]["seed"] = seed
        write(config(seed, "search"), trial)
        write(config(seed, "outer"), dict(outer, search=trial))


def prepare(root, commit):
    require(not (root / "outside-recheck-protocol.json").exists(), "already prepared")
    prior = read(root / "outside-budget-protocol.json")
    summary = read(root / "outside-budget-summary.json")
    progress = read(root / "outside-budget-progress.json")
    require(progress["status"] == "complete"
            and summary["protocol_sha256"] == sha(root / "outside-budget-protocol.json")
            and summary["progress_sha256"] == sha(root / "outside-budget-progress.json")
            and [(p["case_id"], p["rank"], p["action_sha256"]) for p in prior["pairs"]]
            == [(p["case_id"], p["rank"], p["action_sha256"]) for p in summary["pairs"]],
            "original-seed 512 run changed")
    for rank, info in prior["panels"].items():
        require(sha(root / f"inputs/outside-budget-panels/rank-{rank}.json")
                == info["sha256"], f"panel changed: {rank}")
    configs = {}
    for seed in SEEDS:
        search, outer = read(config(seed, "search")), read(config(seed, "outer"))
        require(search["max_moves"] == 23
                and search["simulations_per_move"] == 512
                and search["root_selection"]["candidates"] == 16
                and search["root_selection"]["seed"] == seed
                and outer["search"] == search, f"config differs: {seed}")
        model = root / f"inputs/models/outer-outside-recheck-{seed}"
        model.mkdir(parents=True, exist_ok=True)
        for name in ("state.safetensors", "parameter-order.json"):
            link = model / name
            if not link.exists():
                link.symlink_to(Path("..") / "outer-128" / name)
        write(model / "config.json", outer)
        configs[str(seed)] = {"search": sha(config(seed, "search")),
                              "outer": sha(config(seed, "outer"))}
    protocol = {"schema": "aic-hierarchical-outside-budget-recheck-protocol-v1",
                "experiment_commit": commit,
                "prior_protocol_sha256": sha(root / "outside-budget-protocol.json"),
                "prior_summary_sha256": sha(root / "outside-budget-summary.json"),
                "pairs": prior["pairs"], "panels": prior["panels"],
                "configs": configs, "runner_sha256": sha(Path(__file__)),
                "binary_sha256": sha(Path("target/release/aic-cli")),
                "lower_sha256": sha(root / "inputs/models/lower/state.safetensors"),
                "outer_sha256": sha(root / "inputs/models/outer-128/state.safetensors"),
                "planned_cells": len(SEEDS)*len(prior["panels"])}
    write(root / "outside-recheck-protocol.json", protocol)
    write(root / "outside-recheck-progress.json", {
        "schema": "aic-hierarchical-outside-budget-recheck-progress-v1",
        "protocol_sha256": sha(root / "outside-recheck-protocol.json"),
        "status": "prepared", "records": []})
    print(f"recheck_pairs={len(prior['pairs'])} cells={protocol['planned_cells']}")


def run(root, max_cells):
    protocol = read(root / "outside-recheck-protocol.json")
    progress = read(root / "outside-recheck-progress.json")
    require(progress["protocol_sha256"] == sha(root / "outside-recheck-protocol.json")
            and protocol["prior_protocol_sha256"] == sha(root / "outside-budget-protocol.json")
            and protocol["prior_summary_sha256"] == sha(root / "outside-budget-summary.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["binary_sha256"] == sha(Path("target/release/aic-cli")),
            "protocol inputs changed")
    progress["status"] = "running"
    write(root / "outside-recheck-progress.json", progress)
    added = 0
    for seed in SEEDS:
        model = root / f"inputs/models/outer-outside-recheck-{seed}"
        require(sha(config(seed, "search")) == protocol["configs"][str(seed)]["search"]
                and sha(config(seed, "outer")) == protocol["configs"][str(seed)]["outer"]
                and sha(model / "config.json") == protocol["configs"][str(seed)]["outer"]
                and sha(model / "state.safetensors") == protocol["outer_sha256"],
                f"model changed: {seed}")
        for rank, info in protocol["panels"].items():
            if any(row["seed"] == seed and row["rank"] == int(rank)
                   for row in progress["records"]):
                continue
            panel = root / f"inputs/outside-budget-panels/rank-{rank}.json"
            require(sha(panel) == info["sha256"], f"panel changed: {rank}")
            output = root / f"raw/outside-recheck/{seed}/rank-{rank}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            log = root / f"logs/outside-recheck-{seed}-rank-{rank}.log"
            start = time.monotonic()
            if not output.exists():
                command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                    "--config", str(config(seed, "search")), "--panel", str(panel),
                    "--lower-training-output", str(root / "inputs/models/lower"),
                    "--outer-training-output", str(model),
                    "--catalog-root", "data/game/normalized", "--output", str(output),
                    "--device", "cuda:0"]
                with log.open("w") as stream:
                    result = subprocess.run(command, stdout=stream,
                                            stderr=subprocess.STDOUT, check=False)
                require(result.returncode == 0, f"search failed: {seed}/{rank}; {log}")
            report = read(output)
            require(report["status"] == "complete" and report["partition"] == "training"
                    and report["panel_sha256"] == info["sha256"]
                    and report["lower_checkpoint_sha256"] == protocol["lower_sha256"]
                    and report["outer_checkpoint_sha256"] == protocol["outer_sha256"]
                    and {case["id"] for case in report["cases"]} == set(info["case_ids"]),
                    f"report changed: {seed}/{rank}")
            progress["records"].append({"seed": seed, "rank": int(rank),
                "report_sha256": sha(output), "report_bytes": output.stat().st_size,
                "wall_seconds": time.monotonic()-start})
            write(root / "outside-recheck-progress.json", progress)
            print(f"completed_outside_recheck={seed}/{rank}", flush=True)
            added += 1
            if max_cells is not None and added >= max_cells:
                return
    require(len(progress["records"]) == protocol["planned_cells"], "incomplete")
    progress["status"] = "complete"
    write(root / "outside-recheck-progress.json", progress)


def summarize(root):
    protocol = read(root / "outside-recheck-protocol.json")
    progress = read(root / "outside-recheck-progress.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"], "incomplete")
    trials = {}
    for row in progress["records"]:
        path = root / f"raw/outside-recheck/{row['seed']}/rank-{row['rank']}.json"
        require(sha(path) == row["report_sha256"], f"raw changed: {path}")
        for case in read(path)["cases"]:
            trials.setdefault((case["id"], row["rank"]), []).append({
                "seed": row["seed"],
                "first_state_sha256": case["mcts"]["root_records"][0]["state_sha256"],
                "completed": case["mcts"]["fully_validated"],
                "completed_connections": case["mcts"]["completed_connections"]})
    original = { (row["case_id"], row["rank"]): row for row in
                read(root / "outside-budget-summary.json")["pairs"]}
    pairs = []
    for pair in protocol["pairs"]:
        key = (pair["case_id"], pair["rank"])
        base = sorted(trials[(pair["case_id"], 0)], key=lambda row: row["seed"])
        candidate = sorted(trials[key], key=lambda row: row["seed"])
        require([row["seed"] for row in base] == list(SEEDS)
                and [row["seed"] for row in candidate] == list(SEEDS)
                and all(row["first_state_sha256"] == pair["baseline_state_sha256"]
                        for row in base)
                and all(row["first_state_sha256"] == pair["first_state_sha256"]
                        for row in candidate), f"forced state changed: {key}")
        pairs.append(dict(pair,
            original_seed_512_counts=original[key]["budget512_counts"],
            independent_seed_512_counts=[sum(row["completed"] for row in base),
                                         sum(row["completed"] for row in candidate)],
            baseline_trials=base, candidate_trials=candidate))
    result = {"schema": "aic-hierarchical-outside-budget-recheck-summary-v1",
        "protocol_sha256": sha(root / "outside-recheck-protocol.json"),
        "progress_sha256": sha(root / "outside-recheck-progress.json"),
        "pairs": pairs}
    write(root / "outside-recheck-summary.json", result)
    print({"pairs": len(pairs), "independent_candidate_better": sum(
        p["independent_seed_512_counts"][1] > p["independent_seed_512_counts"][0]
        for p in pairs), "independent_baseline_better": sum(
        p["independent_seed_512_counts"][1] < p["independent_seed_512_counts"][0]
        for p in pairs)})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("write-configs", "prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-cells", type=int)
    args = parser.parse_args()
    if args.operation == "write-configs":
        write_configs()
    elif args.operation == "prepare":
        require(args.experiment_commit, "experiment commit required")
        prepare(args.output, args.experiment_commit)
    elif args.operation == "run":
        run(args.output, args.max_cells)
    else:
        summarize(args.output)
