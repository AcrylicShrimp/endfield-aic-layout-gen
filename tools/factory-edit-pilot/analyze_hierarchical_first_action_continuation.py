"""Summarize paired forced first actions without selecting actions by outcome."""

import argparse
from collections import defaultdict
from pathlib import Path
import statistics

from ppo_harness_state import read, sha, write


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-first-action-analysis: {message}")


def measure(rows):
    return {
        "candidates": len(rows),
        "cases": len({row["case_id"] for row in rows}),
        "better_clear": sum(row["completion_difference"] >= 2 for row in rows),
        "worse_clear": sum(row["completion_difference"] <= -2 for row in rows),
        "unresolved": sum(abs(row["completion_difference"]) < 2 for row in rows),
        "mean_completion_difference_of_three": statistics.mean(
            row["completion_difference"] for row in rows) if rows else None,
        "mean_progress_difference": statistics.mean(
            row["mean_connection_difference"] for row in rows) if rows else None,
        "paired_success_both": sum(row["both_success_seeds"] for row in rows),
        "paired_footprint_difference_mean": statistics.mean(
            difference for row in rows for difference in row["paired_footprint_differences"])
            if any(row["paired_footprint_differences"] for row in rows) else None,
        "paired_transport_difference_mean": statistics.mean(
            difference for row in rows for difference in row["paired_transport_differences"])
            if any(row["paired_transport_differences"] for row in rows) else None,
    }


def analyze(root):
    summary_path = root / "continuation-summary.json"
    summary = read(summary_path)
    progress = read(root / "continuation-progress.json")
    require(summary["progress_sha256"] == sha(root / "continuation-progress.json")
            and progress["status"] == "complete" and len(summary["cases"]) == 48,
            "continuation summary is not final")
    comparisons = []
    for case in summary["cases"]:
        variants = case["variants"]
        baseline = variants[0]
        require("selected-c16-n128" in baseline["reasons"],
                f"rank zero is not the baseline action: {case['case_id']}")
        require(len(baseline["trials"]) == 3, "missing baseline seed")
        for variant in variants[1:]:
            require(len(variant["trials"]) == 3, "missing candidate seed")
            paired_footprint = []
            paired_transport = []
            both = 0
            for candidate_trial, baseline_trial in zip(variant["trials"],
                                                        baseline["trials"]):
                require(candidate_trial["seed"] == baseline_trial["seed"],
                        "paired seeds differ")
                if candidate_trial["completed"] and baseline_trial["completed"]:
                    both += 1
                    paired_footprint.append(candidate_trial["footprint_area"]
                                            - baseline_trial["footprint_area"])
                    paired_transport.append(candidate_trial["transport_cells"]
                                            - baseline_trial["transport_cells"])
            comparisons.append({
                "stream": case["stream"], "case_id": case["case_id"],
                "rank": variant["rank"], "reasons": variant["reasons"],
                "outside_original_16": variant["baseline_visits"] == 0,
                "baseline_completion_count": baseline["completion_count"],
                "candidate_completion_count": variant["completion_count"],
                "completion_difference": variant["completion_count"]
                                          - baseline["completion_count"],
                "mean_connection_difference": variant["mean_completed_connections"]
                                              - baseline["mean_completed_connections"],
                "both_success_seeds": both,
                "paired_footprint_differences": paired_footprint,
                "paired_transport_differences": paired_transport,
                "baseline_q": variant["baseline_q"],
                "baseline_visits": variant["baseline_visits"],
            })
    groups = defaultdict(list)
    for row in comparisons:
        groups["outside" if row["outside_original_16"] else "inside"].append(row)
        for reason in row["reasons"]:
            groups[f"reason:{reason}"].append(row)
    cases = []
    for case in summary["cases"]:
        rows = [row for row in comparisons if row["case_id"] == case["case_id"]]
        outside = [row for row in rows if row["outside_original_16"]]
        inside = [row for row in rows if not row["outside_original_16"]]
        cases.append({
            "stream": case["stream"], "case_id": case["case_id"],
            "baseline_completion_count": case["variants"][0]["completion_count"],
            "outside_candidates": len(outside), "inside_candidates": len(inside),
            "any_outside_clear_better": any(row["completion_difference"] >= 2
                                            for row in outside),
            "any_outside_clear_worse": any(row["completion_difference"] <= -2
                                           for row in outside),
            "any_inside_clear_better": any(row["completion_difference"] >= 2
                                           for row in inside),
            "any_inside_clear_worse": any(row["completion_difference"] <= -2
                                          for row in inside),
        })
    result = {
        "schema": "aic-hierarchical-first-action-analysis-v1",
        "continuation_summary_sha256": sha(summary_path),
        "cases": cases,
        "comparisons": comparisons,
        "groups": {name: measure(rows) for name, rows in sorted(groups.items())},
        "q_ranking": summary["q_ranking"],
        "case_counts": {
            "outside_clear_better": sum(case["any_outside_clear_better"] for case in cases),
            "inside_clear_better": sum(case["any_inside_clear_better"] for case in cases),
            "outside_clear_worse": sum(case["any_outside_clear_worse"] for case in cases),
            "inside_clear_worse": sum(case["any_inside_clear_worse"] for case in cases),
        },
    }
    write(root / "continuation-analysis.json", result)
    print({"cases": len(cases), "comparisons": len(comparisons),
           "case_counts": result["case_counts"],
           "groups": result["groups"]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    args = parser.parse_args()
    analyze(args.output)


if __name__ == "__main__":
    main()
