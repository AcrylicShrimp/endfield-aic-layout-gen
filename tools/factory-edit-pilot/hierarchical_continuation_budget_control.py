"""Compare downstream seed volatility at 128 and 512 simulations."""

import argparse
import itertools
import json
from pathlib import Path
import statistics
import subprocess
import time

from hierarchical_candidate_width import CONFIG_ROOT
from hierarchical_first_action_continuation import SEEDS
from ppo_harness_state import read, sha, write


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-continuation-budget: {message}")


def config(seed, kind):
    return CONFIG_ROOT / f"continuation-budget-512-{seed}-{kind}.json"


def write_configs():
    search = read(CONFIG_ROOT / "c16-n512-search.json")
    outer = read(CONFIG_ROOT / "c16-n512-outer.json")
    require(outer["search"] == search, "source configs differ")
    for seed in SEEDS:
        trial = json.loads(json.dumps(search))
        trial["max_moves"] = 23
        trial["root_selection"]["seed"] = seed
        write(config(seed, "search"), trial)
        write(config(seed, "outer"), dict(outer, search=trial))
    print(f"written_budget_configs={len(SEEDS)}")


def prepare(root, experiment_commit):
    require(not (root / "budget-control-protocol.json").exists(),
            "budget control already prepared")
    progress = read(root / "continuation-progress.json")
    summary = read(root / "continuation-summary.json")
    require(progress["status"] == "complete"
            and summary["progress_sha256"] == sha(root / "continuation-progress.json"),
            "primary continuation is not final")
    lower = root / "inputs/models/lower/state.safetensors"
    outer = root / "inputs/models/outer-128/state.safetensors"
    require(lower.is_file() and outer.is_file(), "source checkpoints missing")
    hashes = {}
    for seed in SEEDS:
        search = read(config(seed, "search"))
        model_config = read(config(seed, "outer"))
        require(search["simulations_per_move"] == 512
                and search["max_moves"] == 23
                and search["root_selection"]["candidates"] == 16
                and search["root_selection"]["seed"] == seed
                and model_config["search"] == search,
                f"budget config differs: {seed}")
        directory = root / "inputs/models" / f"outer-budget-512-{seed}"
        directory.mkdir(parents=True, exist_ok=True)
        for name in ("state.safetensors", "parameter-order.json"):
            link = directory / name
            if not link.exists():
                link.symlink_to(Path("..") / "outer-128" / name)
        write(directory / "config.json", model_config)
        hashes[str(seed)] = {
            "search": sha(config(seed, "search")),
            "outer": sha(config(seed, "outer")),
            "model_config": sha(directory / "config.json"),
        }
    protocol = {
        "schema": "aic-hierarchical-continuation-budget-protocol-v1",
        "experiment_commit": experiment_commit,
        "primary_progress_sha256": sha(root / "continuation-progress.json"),
        "primary_summary_sha256": sha(root / "continuation-summary.json"),
        "panel_sha256": sha(root / "inputs/continuation-panels/rank-0.json"),
        "binary_sha256": sha(Path("target/release/aic-cli")),
        "runner_sha256": sha(Path(__file__)),
        "lower_checkpoint_sha256": sha(lower),
        "outer_checkpoint_sha256": sha(outer),
        "configs": hashes, "planned_cells": len(SEEDS),
    }
    write(root / "budget-control-protocol.json", protocol)
    write(root / "budget-control-progress.json", {
        "schema": "aic-hierarchical-continuation-budget-progress-v1",
        "protocol_sha256": sha(root / "budget-control-protocol.json"),
        "records": [], "status": "prepared"})
    print(f"prepared_budget_control={len(SEEDS)}")


def run(root, max_cells):
    protocol = read(root / "budget-control-protocol.json")
    progress = read(root / "budget-control-progress.json")
    require(progress["protocol_sha256"] == sha(root / "budget-control-protocol.json")
            and protocol["primary_progress_sha256"]
            == sha(root / "continuation-progress.json")
            and protocol["primary_summary_sha256"]
            == sha(root / "continuation-summary.json")
            and protocol["panel_sha256"]
            == sha(root / "inputs/continuation-panels/rank-0.json")
            and protocol["binary_sha256"] == sha(Path("target/release/aic-cli"))
            and protocol["runner_sha256"] == sha(Path(__file__)),
            "protocol inputs changed")
    progress["status"] = "running"
    write(root / "budget-control-progress.json", progress)
    completed = 0
    for seed in SEEDS:
        if any(row["seed"] == seed for row in progress["records"]):
            continue
        search_config = config(seed, "search")
        outer_config = config(seed, "outer")
        model = root / "inputs/models" / f"outer-budget-512-{seed}"
        hashes = protocol["configs"][str(seed)]
        require(sha(search_config) == hashes["search"]
                and sha(outer_config) == hashes["outer"]
                and sha(model / "config.json") == hashes["model_config"]
                and sha(model / "state.safetensors")
                == protocol["outer_checkpoint_sha256"],
                f"budget model changed: {seed}")
        output = root / "raw/budget-control" / str(seed) / "rank-0.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        log = root / "logs" / f"budget-control-{seed}.log"
        started = time.monotonic()
        if not output.exists():
            command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                       "--config", str(search_config),
                       "--panel", str(root / "inputs/continuation-panels/rank-0.json"),
                       "--lower-training-output", str(root / "inputs/models/lower"),
                       "--outer-training-output", str(model),
                       "--catalog-root", "data/game/normalized",
                       "--output", str(output), "--device", "cuda:0"]
            with log.open("w") as stream:
                result = subprocess.run(command, stdout=stream,
                                        stderr=subprocess.STDOUT, check=False)
            require(result.returncode == 0, f"budget control failed: {seed}")
        report = read(output)
        require(report["status"] == "complete" and report["partition"] == "training"
                and report["panel_sha256"] == protocol["panel_sha256"]
                and report["lower_checkpoint_sha256"]
                == protocol["lower_checkpoint_sha256"]
                and report["outer_checkpoint_sha256"]
                == protocol["outer_checkpoint_sha256"]
                and len(report["cases"]) == 48,
                f"budget control report differs: {seed}")
        progress["records"].append({"seed": seed,
            "report_sha256": sha(output), "report_bytes": output.stat().st_size,
            "wall_seconds": time.monotonic() - started})
        write(root / "budget-control-progress.json", progress)
        print(f"completed_budget_control={seed}", flush=True)
        completed += 1
        if max_cells is not None and completed >= max_cells:
            return
    require(len(progress["records"]) == len(SEEDS), "budget control incomplete")
    progress["status"] = "complete"
    write(root / "budget-control-progress.json", progress)


def summarize(root):
    protocol = read(root / "budget-control-protocol.json")
    progress = read(root / "budget-control-progress.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == len(SEEDS),
            "budget control incomplete")
    outcomes = {}
    state_hashes = {}
    progress_values = {}
    for budget, directory in ((128, "continuation"), (512, "budget-control")):
        outcomes[budget] = {}
        state_hashes[budget] = {}
        progress_values[budget] = {}
        for seed in SEEDS:
            path = root / "raw" / directory / str(seed) / "rank-0.json"
            record = next(row for row in (
                read(root / "continuation-progress.json")["records"]
                if budget == 128 else progress["records"])
                if row["seed"] == seed and (budget == 512 or row["rank"] == 0))
            require(sha(path) == record["report_sha256"],
                    f"report hash differs: {budget}/{seed}")
            report = read(path)
            outcomes[budget][seed] = {case["id"]: case["mcts"]["fully_validated"]
                                      for case in report["cases"]}
            state_hashes[budget][seed] = {
                case["id"]: case["mcts"]["root_records"][0]["state_sha256"]
                for case in report["cases"]}
            progress_values[budget][seed] = {
                case["id"]: case["mcts"]["completed_connections"]
                for case in report["cases"]}
    cases = sorted(outcomes[128][SEEDS[0]])
    require(len(cases) == 48 and all(set(outcomes[budget][seed]) == set(cases)
                                     for budget in outcomes for seed in SEEDS),
            "case identities differ")
    require(all(len({state_hashes[budget][seed][case]
                     for budget in outcomes for seed in SEEDS}) == 1
                for case in cases), "forced first state differs")
    result = {"schema": "aic-hierarchical-continuation-budget-summary-v1",
              "protocol_sha256": sha(root / "budget-control-protocol.json"),
              "progress_sha256": sha(root / "budget-control-progress.json"),
              "budgets": {}}
    for budget in (128, 512):
        pairs = {}
        for left, right in itertools.combinations(SEEDS, 2):
            pairs[f"{left}-{right}"] = sum(outcomes[budget][left][case]
                                            != outcomes[budget][right][case]
                                            for case in cases)
        result["budgets"][str(budget)] = {
            "successes_by_seed": {str(seed): sum(outcomes[budget][seed].values())
                                  for seed in SEEDS},
            "seed_pair_disagreements": pairs,
            "mean_pair_disagreement": statistics.mean(pairs.values()),
            "case_success_counts": {case: sum(outcomes[budget][seed][case]
                                              for seed in SEEDS) for case in cases},
            "case_mean_completed_connections": {
                case: statistics.mean(progress_values[budget][seed][case]
                                      for seed in SEEDS) for case in cases},
        }
    write(root / "budget-control-summary.json", result)
    print({budget: {k: v for k, v in result["budgets"][budget].items()
                    if k in ("successes_by_seed", "seed_pair_disagreements",
                             "mean_pair_disagreement")}
           for budget in ("128", "512")})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("write-configs", "prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-cells", type=int)
    args = parser.parse_args()
    if args.operation == "write-configs":
        write_configs()
    elif args.operation == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    elif args.operation == "run":
        run(args.output, args.max_cells)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
