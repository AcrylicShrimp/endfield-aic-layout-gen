"""Measure search-target concentration in retained horizon-12 collections."""

import argparse
import math
from pathlib import Path
import statistics

from hierarchical_horizon_onpolicy import require, validate
from ppo_harness_state import read, sha, write


def root_metrics(root):
    actions = root["actions"]
    require(actions and 0 <= root["selected_action"] < len(actions),
            "invalid root actions or selection")
    target = [action["policy_target"] for action in actions]
    require(all(value is not None and math.isfinite(value) and value >= 0
                for value in target)
            and abs(sum(target) - 1) < 1e-6,
            "invalid policy target")
    selected = root["selected_action"]
    visited = [action["visits"] > 0 for action in actions]
    entropy = -sum(value * math.log(value) for value in target if value > 0)
    return {
        "actions": len(actions),
        "visited": sum(visited),
        "unvisited_mass": sum(value for value, seen in zip(target, visited)
                              if not seen),
        "selected_mass": target[selected],
        "effective_action_fraction": math.exp(entropy) / len(actions),
        "top_target_visited": visited[max(range(len(actions)),
                                       key=lambda index: target[index])],
    }


def mean(rows, key):
    return statistics.mean(row[key] for row in rows) if rows else None


def audit(root):
    protocol, progress, _ = validate(root)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "on-policy run incomplete")
    result_rows = []
    for record in progress["records"]:
        name = record["name"]
        path = root / "collections" / f"{name}.json"
        collection_sha = sha(path)
        require(collection_sha == record["collection_sha256"],
                f"collection changed: {name}")
        report = read(path)
        require(report["status"] == "complete" and len(report["cases"]) == 32,
                f"collection incomplete: {name}")
        require(sum(bool(case["mcts"]["fully_validated"])
                    for case in report["cases"])
                == record["collection_completed"],
                f"collection outcomes changed: {name}")
        first = []
        all_roots = []
        for case in report["cases"]:
            roots = case["mcts"]["root_records"]
            if not roots:
                continue
            metrics = [root_metrics(item) for item in roots]
            first.append(dict(metrics[0], completed=bool(case["mcts"][
                "fully_validated"])))
            all_roots.extend(metrics)
        require(first, f"no covered first roots: {name}")
        result_rows.append({
            "name": name,
            "collection_sha256": collection_sha,
            "cases": len(report["cases"]),
            "covered_first_roots": len(first),
            "covered_all_roots": len(all_roots),
            "completed": record["collection_completed"],
            "first_mean_actions": mean(first, "actions"),
            "first_mean_visited": mean(first, "visited"),
            "first_mean_unvisited_mass": mean(first, "unvisited_mass"),
            "first_mean_selected_mass": mean(first, "selected_mass"),
            "first_mean_effective_action_fraction": mean(
                first, "effective_action_fraction"),
            "first_top_target_visited": sum(row["top_target_visited"] for row in first),
            "first_success_mean_unvisited_mass": mean(
                [row for row in first if row["completed"]], "unvisited_mass"),
            "first_failure_mean_unvisited_mass": mean(
                [row for row in first if not row["completed"]], "unvisited_mass"),
            "all_mean_unvisited_mass": mean(all_roots, "unvisited_mass"),
            "all_mean_selected_mass": mean(all_roots, "selected_mass"),
        })
        print(f"audited={name} first_roots={len(first)} "
              f"mean_unvisited={result_rows[-1]['first_mean_unvisited_mass']:.4f}",
              flush=True)
    write(root / "target-quality-summary.json", {
        "schema": "aic-hierarchical-horizon-target-quality-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "rows": result_rows,
    })


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    audit(args.output)
