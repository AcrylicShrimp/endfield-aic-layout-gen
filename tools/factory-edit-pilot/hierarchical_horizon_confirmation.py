"""Confirm a screened search horizon on a fresh, disjoint panel."""

import argparse
from pathlib import Path
import statistics
import subprocess
import time

from hierarchical_recovery_search import BINARY, SEEDS, action_digest, name
from hierarchical_target_learner import SAMPLING, identity, panel
from ppo_harness_state import read, sha, write


SOURCE = Path("artifacts/recovery-search-v2")
ARMS = ("base", "horizon12")


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-horizon-confirmation: {message}")


def config(arm, seed):
    return SOURCE / "configs" / f"{name(arm, seed)}-search.json"


def model(arm, seed):
    return SOURCE / "models" / name(arm, seed)


def prepare(root, commit):
    require(not root.exists(), "confirmation output exists")
    screen = read(SOURCE / "summary.json")
    require(screen["summary"]["horizon12"]["passes_screen"],
            "horizon 12 did not pass the frozen screen gate")
    seen = set()
    for stream in ("s1", "s2", "s3"):
        source = read(SOURCE / "inputs/panels" / f"{stream}.json")
        for case in source["cases"]:
            key = identity(case)
            require(key not in seen, "duplicate screen problem")
            seen.add(key)
    root.mkdir(parents=True)
    panel_sha = panel(root, "confirmation", 20262560, 512, 64, seen)
    configs = {}
    for seed in SEEDS:
        for arm in ARMS:
            search = read(config(arm, seed))
            require(search["horizon"] == (12 if arm == "horizon12" else 6)
                    and search["root_selection"]["seed"] == seed
                    and search["simulations_per_move"] == 128,
                    "search configuration differs")
            configs[name(arm, seed)] = {
                "search": sha(config(arm, seed)),
                "model_config": sha(model(arm, seed) / "config.json"),
            }
    protocol = {
        "schema": "aic-hierarchical-horizon-confirmation-protocol-v1",
        "experiment_commit": commit,
        "runner_sha256": sha(Path(__file__)),
        "screen_summary_sha256": sha(SOURCE / "summary.json"),
        "binary_sha256": sha(BINARY),
        "sampling_sha256": sha(SAMPLING),
        "panel_sha256": panel_sha,
        "lower_sha256": sha(SOURCE / "inputs/models/lower/state.safetensors"),
        "outer_sha256": sha(SOURCE / "inputs/models/outer-128/state.safetensors"),
        "configs": configs, "planned_cells": len(SEEDS) * len(ARMS),
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {
        "schema": "aic-hierarchical-horizon-confirmation-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared_confirmation=64 cases=6 cells disjoint_from_screen=48")


def validate(root):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["screen_summary_sha256"] == sha(SOURCE / "summary.json")
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["sampling_sha256"] == sha(SAMPLING)
            and protocol["panel_sha256"] == sha(root / "panels/confirmation.json")
            and protocol["lower_sha256"]
            == sha(SOURCE / "inputs/models/lower/state.safetensors")
            and protocol["outer_sha256"]
            == sha(SOURCE / "inputs/models/outer-128/state.safetensors"),
            "protocol input changed")
    for seed in SEEDS:
        for arm in ARMS:
            hashes = protocol["configs"][name(arm, seed)]
            require(hashes["search"] == sha(config(arm, seed))
                    and hashes["model_config"] == sha(model(arm, seed) / "config.json")
                    and protocol["outer_sha256"]
                    == sha(model(arm, seed) / "state.safetensors"),
                    "model or search config changed")
    return protocol, progress


def run(root, max_cells):
    protocol, progress = validate(root)
    progress["status"] = "running"
    write(root / "progress.json", progress)
    count = 0
    for seed in SEEDS:
        for arm in ARMS:
            if any(row["seed"] == seed and row["arm"] == arm
                   for row in progress["records"]):
                continue
            if max_cells is not None and count >= max_cells:
                return
            output = root / "raw" / f"{name(arm, seed)}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            log = root / "logs" / f"{name(arm, seed)}.log"
            log.parent.mkdir(parents=True, exist_ok=True)
            seconds = None
            if not output.exists():
                command = [str(BINARY), "learning", "hierarchical-search-harness",
                           "--config", str(config(arm, seed)),
                           "--panel", str(root / "panels/confirmation.json"),
                           "--lower-training-output", str(SOURCE / "inputs/models/lower"),
                           "--outer-training-output", str(model(arm, seed)),
                           "--catalog-root", "data/game/normalized",
                           "--output", str(output), "--device", "cuda:0"]
                started = time.monotonic()
                with log.open("w") as handle:
                    result = subprocess.run(command, stdout=handle,
                                            stderr=subprocess.STDOUT, check=False)
                require(result.returncode == 0, f"search failed: {arm}/{seed}; see {log}")
                seconds = time.monotonic() - started
            report = read(output)
            require(report["status"] == "complete"
                    and report["panel_sha256"] == protocol["panel_sha256"]
                    and report["lower_checkpoint_sha256"] == protocol["lower_sha256"]
                    and report["outer_checkpoint_sha256"] == protocol["outer_sha256"]
                    and len(report["cases"]) == 64,
                    "confirmation report lineage differs")
            progress["records"].append({"arm": arm, "seed": seed,
                                        "report_sha256": sha(output),
                                        "report_bytes": output.stat().st_size,
                                        "wall_seconds": seconds})
            write(root / "progress.json", progress)
            count += 1
            print(f"confirmation={arm}/{seed} completed="
                  f"{sum(c['mcts']['fully_validated'] for c in report['cases'])}/64 "
                  f"seconds={seconds}", flush=True)
    require(len(progress["records"]) == protocol["planned_cells"], "incomplete")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def summarize(root):
    protocol, progress = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"],
            "confirmation incomplete")
    reports = {}
    for arm in ARMS:
        for seed in SEEDS:
            path = root / "raw" / f"{name(arm, seed)}.json"
            record = next(row for row in progress["records"]
                          if row["arm"] == arm and row["seed"] == seed)
            require(sha(path) == record["report_sha256"], "report changed")
            reports[(arm, seed)] = read(path)
    cases = []
    for index in range(64):
        entry = {"case_id": reports[("base", SEEDS[0])]["cases"][index]["id"],
                 "arms": {}}
        for arm in ARMS:
            entry["arms"][arm] = {}
            for seed in SEEDS:
                case = reports[(arm, seed)]["cases"][index]
                base = reports[("base", seed)]["cases"][index]
                root_record = case["mcts"]["root_records"][0]
                reference = base["mcts"]["root_records"][0]
                require(case["id"] == entry["case_id"]
                        and root_record["state_sha256"] == reference["state_sha256"]
                        and [action_digest(a) for a in root_record["actions"]]
                        == [action_digest(a) for a in reference["actions"]],
                        "first-root witness differs")
                entry["arms"][arm][str(seed)] = {
                    "success": case["mcts"]["fully_validated"],
                    "completed_connections": case["mcts"]["completed_connections"],
                    "first_action_sha256": action_digest(
                        root_record["actions"][root_record["selected_action"]]),
                }
        cases.append(entry)
    completed = {arm: [sum(row["arms"][arm][str(seed)]["success"] for row in cases)
                       for seed in SEEDS] for arm in ARMS}
    gains = [sum(row["arms"]["horizon12"][str(seed)]["success"]
                 and not row["arms"]["base"][str(seed)]["success"] for row in cases)
             for seed in SEEDS]
    losses = [sum(row["arms"]["base"][str(seed)]["success"]
                  and not row["arms"]["horizon12"][str(seed)]["success"] for row in cases)
              for seed in SEEDS]
    base_robust = [row for row in cases if sum(
        value["success"] for value in row["arms"]["base"].values()) >= 2]
    lost_robust = sum(sum(value["success"] for value in row["arms"]["horizon12"].values()) < 2
                      for row in base_robust)
    delta = [a - b for a, b in zip(completed["horizon12"], completed["base"])]
    result = {
        "schema": "aic-hierarchical-horizon-confirmation-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "completed_per_seed": completed,
        "paired_gains_per_seed": gains, "paired_losses_per_seed": losses,
        "delta_per_seed": delta, "mean_delta": statistics.mean(delta),
        "baseline_robust_successes": len(base_robust),
        "lost_baseline_robust_successes": lost_robust,
        "first_actions_equal_base_per_seed": [sum(
            row["arms"]["base"][str(seed)]["first_action_sha256"]
            == row["arms"]["horizon12"][str(seed)]["first_action_sha256"]
            for row in cases) for seed in SEEDS],
        "wall_seconds_by_arm": {arm: sum(row["wall_seconds"] or 0
                                           for row in progress["records"]
                                           if row["arm"] == arm) for arm in ARMS},
        "passes_gate": (statistics.mean(delta) >= 4
                        and sum(value > 0 for value in delta) >= 2
                        and lost_robust <= 3),
        "cases": cases,
    }
    write(root / "summary.json", result)
    print(f"completed={completed} delta={delta} gains={gains} losses={losses} "
          f"lost_robust={lost_robust} gate={result['passes_gate']}")


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
