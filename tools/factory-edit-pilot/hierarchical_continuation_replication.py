"""Replicate primary first-action winners under independent downstream seeds."""

import argparse
import itertools
import json
from pathlib import Path
import subprocess
import time

from hierarchical_candidate_width import CONFIG_ROOT
from ppo_harness_state import read, sha, write


SEEDS = (20260925, 20260926, 20260927)


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-continuation-replication: {message}")


def config(seed, kind):
    return CONFIG_ROOT / f"continuation-replication-{seed}-{kind}.json"


def write_configs():
    search = read(CONFIG_ROOT / "continuation-20260922-search.json")
    outer = read(CONFIG_ROOT / "continuation-20260922-outer.json")
    require(outer["search"] == search, "source configs differ")
    for seed in SEEDS:
        trial = json.loads(json.dumps(search))
        trial["root_selection"]["seed"] = seed
        write(config(seed, "search"), trial)
        write(config(seed, "outer"), dict(outer, search=trial))
    print(f"written_replication_configs={len(SEEDS)}")


def primary_pairs(variants):
    for left, right in itertools.combinations(range(len(variants)), 2):
        difference = variants[left]["completion_count"] - variants[right]["completion_count"]
        if abs(difference) >= 2:
            yield (left, right) if difference > 0 else (right, left)


def prepare(root, experiment_commit):
    require(not (root / "replication-protocol.json").exists(),
            "replication already prepared")
    primary = read(root / "continuation-summary.json")
    progress = read(root / "continuation-progress.json")
    require(primary["progress_sha256"] == sha(root / "continuation-progress.json")
            and progress["status"] == "complete" and len(primary["cases"]) == 48,
            "primary continuation is not final")
    triggered = {case["case_id"] for case in primary["cases"]
                 if any(primary_pairs(case["variants"]))}
    require(triggered, "no primary clear pairs to replicate")
    panels = {}
    for rank in read(root / "continuation-protocol.json")["panels"]:
        source = read(root / "inputs/continuation-panels" / f"rank-{rank}.json")
        source["cases"] = [case for case in source["cases"] if case["id"] in triggered]
        if not source["cases"]:
            continue
        output = root / "inputs/replication-panels" / f"rank-{rank}.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        write(output, source)
        panels[rank] = {"sha256": sha(output), "cases": len(source["cases"])}
    hashes = {}
    for seed in SEEDS:
        search = read(config(seed, "search"))
        outer = read(config(seed, "outer"))
        require(search["simulations_per_move"] == 128
                and search["max_moves"] == 23
                and search["root_selection"]["candidates"] == 16
                and search["root_selection"]["seed"] == seed
                and outer["search"] == search,
                f"replication config differs: {seed}")
        model = root / "inputs/models" / f"outer-replication-{seed}"
        model.mkdir(parents=True, exist_ok=True)
        for name in ("state.safetensors", "parameter-order.json"):
            link = model / name
            if not link.exists():
                link.symlink_to(Path("..") / "outer-128" / name)
        write(model / "config.json", outer)
        hashes[str(seed)] = {"search": sha(config(seed, "search")),
                             "outer": sha(config(seed, "outer")),
                             "model_config": sha(model / "config.json")}
    protocol = {
        "schema": "aic-hierarchical-continuation-replication-protocol-v1",
        "experiment_commit": experiment_commit,
        "primary_summary_sha256": sha(root / "continuation-summary.json"),
        "triggered_cases": len(triggered),
        "triggered_case_ids": sorted(triggered),
        "panels": panels, "configs": hashes,
        "binary_sha256": sha(Path("target/release/aic-cli")),
        "runner_sha256": sha(Path(__file__)),
        "lower_checkpoint_sha256": sha(root / "inputs/models/lower/state.safetensors"),
        "outer_checkpoint_sha256": sha(root / "inputs/models/outer-128/state.safetensors"),
        "planned_cells": len(SEEDS) * len(panels),
    }
    write(root / "replication-protocol.json", protocol)
    write(root / "replication-progress.json", {
        "schema": "aic-hierarchical-continuation-replication-progress-v1",
        "protocol_sha256": sha(root / "replication-protocol.json"),
        "records": [], "status": "prepared"})
    print(f"triggered_cases={len(triggered)} ranks={len(panels)} "
          f"cells={protocol['planned_cells']}")


def run(root, max_cells):
    protocol = read(root / "replication-protocol.json")
    progress = read(root / "replication-progress.json")
    require(progress["protocol_sha256"] == sha(root / "replication-protocol.json")
            and protocol["primary_summary_sha256"] == sha(root / "continuation-summary.json")
            and protocol["binary_sha256"] == sha(Path("target/release/aic-cli"))
            and protocol["runner_sha256"] == sha(Path(__file__)),
            "replication protocol changed")
    for rank, info in protocol["panels"].items():
        require(sha(root / "inputs/replication-panels" / f"rank-{rank}.json")
                == info["sha256"], f"replication panel changed: {rank}")
    progress["status"] = "running"
    write(root / "replication-progress.json", progress)
    completed = 0
    for seed in SEEDS:
        hashes = protocol["configs"][str(seed)]
        model = root / "inputs/models" / f"outer-replication-{seed}"
        require(sha(config(seed, "search")) == hashes["search"]
                and sha(config(seed, "outer")) == hashes["outer"]
                and sha(model / "config.json") == hashes["model_config"]
                and sha(model / "state.safetensors")
                == protocol["outer_checkpoint_sha256"],
                f"replication model changed: {seed}")
        for rank, info in protocol["panels"].items():
            if any(row["seed"] == seed and row["rank"] == int(rank)
                   for row in progress["records"]):
                continue
            output = root / "raw/replication" / str(seed) / f"rank-{rank}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            log = root / "logs" / f"replication-{seed}-rank-{rank}.log"
            started = time.monotonic()
            if not output.exists():
                command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                           "--config", str(config(seed, "search")),
                           "--panel", str(root / "inputs/replication-panels" / f"rank-{rank}.json"),
                           "--lower-training-output", str(root / "inputs/models/lower"),
                           "--outer-training-output", str(model),
                           "--catalog-root", "data/game/normalized",
                           "--output", str(output), "--device", "cuda:0"]
                with log.open("w") as stream:
                    result = subprocess.run(command, stdout=stream,
                                            stderr=subprocess.STDOUT, check=False)
                require(result.returncode == 0, f"replication failed: {seed}/{rank}")
            report = read(output)
            require(report["status"] == "complete" and report["partition"] == "training"
                    and report["panel_sha256"] == info["sha256"]
                    and report["lower_checkpoint_sha256"]
                    == protocol["lower_checkpoint_sha256"]
                    and report["outer_checkpoint_sha256"]
                    == protocol["outer_checkpoint_sha256"]
                    and len(report["cases"]) == info["cases"],
                    f"replication report differs: {seed}/{rank}")
            progress["records"].append({"seed": seed, "rank": int(rank),
                "report_sha256": sha(output), "report_bytes": output.stat().st_size,
                "wall_seconds": time.monotonic() - started})
            write(root / "replication-progress.json", progress)
            print(f"completed_replication={seed}/{rank}", flush=True)
            completed += 1
            if max_cells is not None and completed >= max_cells:
                return
    require(len(progress["records"]) == protocol["planned_cells"],
            "replication incomplete")
    progress["status"] = "complete"
    write(root / "replication-progress.json", progress)


def summarize(root):
    protocol = read(root / "replication-protocol.json")
    progress = read(root / "replication-progress.json")
    primary = read(root / "continuation-summary.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"]
            and protocol["primary_summary_sha256"] == sha(root / "continuation-summary.json"),
            "replication is not final")
    by_id = {case["case_id"]: case for case in primary["cases"]}
    trials = {case_id: {} for case_id in protocol["triggered_case_ids"]}
    for seed in SEEDS:
        for rank in protocol["panels"]:
            path = root / "raw/replication" / str(seed) / f"rank-{rank}.json"
            record = next(row for row in progress["records"]
                          if row["seed"] == seed and row["rank"] == int(rank))
            require(sha(path) == record["report_sha256"],
                    f"replication report changed: {seed}/{rank}")
            report = read(path)
            for case in report["cases"]:
                case_id = case["id"]
                expected = by_id[case_id]["variants"][int(rank)]
                state = case["mcts"]["root_records"][0]["state_sha256"]
                require(state == expected["trials"][0]["first_state_sha256"],
                        f"forced state changed: {case_id}/{rank}/{seed}")
                trials[case_id].setdefault(int(rank), []).append({
                    "seed": seed, "completed": case["mcts"]["fully_validated"],
                    "completed_connections": case["mcts"]["completed_connections"],
                })
    cases = []
    total_primary = total_confirmed = 0
    for case_id in protocol["triggered_case_ids"]:
        variants = by_id[case_id]["variants"]
        counts = {}
        for rank, variant in enumerate(variants):
            require(len(trials[case_id][rank]) == len(SEEDS),
                    f"replication missing: {case_id}/{rank}")
            counts[rank] = sum(row["completed"] for row in trials[case_id][rank])
        confirmed = []
        primary_pairs_here = list(primary_pairs(variants))
        for winner, loser in primary_pairs_here:
            if counts[winner] - counts[loser] >= 2:
                confirmed.append([winner, loser])
        total_primary += len(primary_pairs_here)
        total_confirmed += len(confirmed)
        cases.append({"stream": by_id[case_id]["stream"], "case_id": case_id,
                      "replication_completion_counts": [counts[i] for i in range(len(variants))],
                      "primary_clear_pairs": len(primary_pairs_here),
                      "confirmed_pairs": confirmed})
    result = {
        "schema": "aic-hierarchical-continuation-replication-summary-v1",
        "protocol_sha256": sha(root / "replication-protocol.json"),
        "progress_sha256": sha(root / "replication-progress.json"),
        "triggered_cases": len(cases),
        "primary_clear_pairs": total_primary,
        "confirmed_pairs": total_confirmed,
        "cases": cases,
    }
    write(root / "replication-summary.json", result)
    print(f"triggered_cases={len(cases)} primary_pairs={total_primary} "
          f"confirmed_pairs={total_confirmed}")


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
