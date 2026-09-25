"""Audit first-root target concentration after restored lower-model training."""

import argparse
from pathlib import Path
import statistics

from hierarchical_horizon_onpolicy import require
from hierarchical_horizon_target_audit import root_metrics
from hierarchical_lower_restored import validate
from hierarchical_target_learner import STREAMS
from ppo_harness_state import read, sha, write


def average(rows, key):
    return statistics.mean(row[key] for row in rows)


def audit(root):
    protocol, progress = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "integrated training has not completed")
    rows = []
    first_by_arm = {"h6": [], "h12": []}
    for record in progress["records"]:
        if record["arm"] == "mixed":
            continue
        path = root / "collections" / f"{record['name']}.json"
        require(sha(path) == record["collection_sha256"],
                f"collection changed: {record['name']}")
        report = read(path)
        require(report["status"] == "complete" and len(report["cases"]) == 32
                and sum(bool(case["mcts"]["fully_validated"])
                        for case in report["cases"])
                == record["collection_completed"],
                f"collection results changed: {record['name']}")
        first = [root_metrics(case["mcts"]["root_records"][0])
                 for case in report["cases"]
                 if case["mcts"]["root_records"]]
        require(first, f"no first roots: {record['name']}")
        first_by_arm[record["arm"]].extend(first)
        rows.append({
            "name": record["name"], "stream": record["stream"],
            "generation": record["generation"], "arm": record["arm"],
            "collection_sha256": record["collection_sha256"],
            "first_roots": len(first),
            "mean_legal_actions": average(first, "actions"),
            "mean_visited_actions": average(first, "visited"),
            "mean_unvisited_target_mass": average(first, "unvisited_mass"),
            "mean_selected_target_mass": average(first, "selected_mass"),
            "mean_effective_action_fraction": average(
                first, "effective_action_fraction"),
            "top_target_visited": sum(row["top_target_visited"] for row in first),
        })
    require(len(rows) == len(STREAMS) * 3 * 2,
            "expected both independently collected search arms")
    by_arm = {}
    for arm in ("h6", "h12"):
        selected = first_by_arm[arm]
        by_arm[arm] = {
            "first_roots": len(selected),
            "mean_unvisited_target_mass": average(selected, "unvisited_mass"),
            "mean_selected_target_mass": average(selected, "selected_mass"),
            "mean_effective_action_fraction": average(
                selected, "effective_action_fraction"),
        }
    output = {
        "schema": "aic-hierarchical-lower-restored-target-audit-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "runner_sha256": sha(Path(__file__)),
        "root_metrics_source_sha256": sha(
            Path(__file__).with_name("hierarchical_horizon_target_audit.py")),
        "by_arm": by_arm,
        "rows": rows,
    }
    write(root / "target-quality-summary.json", output)
    print("first-root target audit complete:", by_arm)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    audit(args.output)
