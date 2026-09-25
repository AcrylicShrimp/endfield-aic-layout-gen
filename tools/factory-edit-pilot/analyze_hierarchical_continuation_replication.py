"""Analyze independently confirmed first-action pairs and conditional quality."""

import argparse
from collections import defaultdict
import itertools
from pathlib import Path
import statistics

from hierarchical_candidate_width import ARMS, label
from hierarchical_continuation_replication import SEEDS as REPLICATION_SEEDS
from ppo_harness_state import read, sha, write


PRIMARY_SEEDS = (20260922, 20260923, 20260924)


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-continuation-replication-analysis: {message}")


def analyze(root):
    primary = read(root / "continuation-summary.json")
    replication = read(root / "replication-summary.json")
    progress = read(root / "replication-progress.json")
    require(replication["progress_sha256"] == sha(root / "replication-progress.json")
            and progress["status"] == "complete"
            and read(root / "replication-protocol.json")["primary_summary_sha256"]
            == sha(root / "continuation-summary.json"),
            "replication is not final")
    by_id = {case["case_id"]: case for case in primary["cases"]}
    repeated = {case["case_id"]: case for case in replication["cases"]}
    require(len(by_id) == 48 and len(repeated) == replication["triggered_cases"],
            "case identity differs")
    trial = {case_id: {rank: {} for rank in range(len(by_id[case_id]["variants"]))}
             for case_id in repeated}
    for case_id in repeated:
        for variant in by_id[case_id]["variants"]:
            for row in variant["trials"]:
                require(row["seed"] in PRIMARY_SEEDS, "primary seed differs")
                trial[case_id][variant["rank"]][row["seed"]] = row
    for seed in REPLICATION_SEEDS:
        for rank in read(root / "replication-protocol.json")["panels"]:
            path = root / "raw/replication" / str(seed) / f"rank-{rank}.json"
            record = next(row for row in progress["records"]
                          if row["seed"] == seed and row["rank"] == int(rank))
            require(sha(path) == record["report_sha256"],
                    f"replication raw changed: {seed}/{rank}")
            for case in read(path)["cases"]:
                result = case["mcts"]
                require(case["id"] in trial and seed not in trial[case["id"]][int(rank)],
                        f"replication case repeats: {seed}/{rank}/{case['id']}")
                trial[case["id"]][int(rank)][seed] = {
                    "seed": seed, "completed": result["fully_validated"],
                    "completed_connections": result["completed_connections"],
                    "footprint_area": result["footprint_area"],
                    "transport_cells": result["transport_cells"],
                    "turns": result["turns"],
                }
    confirmed_groups = defaultdict(list)
    q_rank = {label(c, n): {"pairs": 0, "cases": set(), "concordant": 0,
                            "discordant": 0, "tied": 0, "outside_winners": 0}
              for c, n in ARMS}
    cases = []
    quality = {"paired_success_seed_samples": 0, "cases": set(),
               "q_higher_smaller_area": 0, "q_higher_larger_area": 0,
               "equal_area": 0, "area_differences": [],
               "transport_differences": []}
    for case_id, repeat in repeated.items():
        source = by_id[case_id]
        variants = source["variants"]
        require(all(set(trial[case_id][rank]) == set(PRIMARY_SEEDS + REPLICATION_SEEDS)
                    for rank in trial[case_id]),
                f"replication trials missing: {case_id}")
        confirmed = [tuple(pair) for pair in repeat["confirmed_pairs"]]
        candidate_winners = []
        for winner_rank, loser_rank in confirmed:
            winner, loser = variants[winner_rank], variants[loser_rank]
            outside = winner["baseline_visits"] == 0
            for arm, metric in q_rank.items():
                wq, lq = winner["search_evidence"][arm], loser["search_evidence"][arm]
                if wq["visits"] == 0 or lq["visits"] == 0:
                    continue
                metric["pairs"] += 1
                metric["cases"].add(case_id)
                metric["concordant"] += wq["q"] > lq["q"]
                metric["discordant"] += wq["q"] < lq["q"]
                metric["tied"] += wq["q"] == lq["q"]
                metric["outside_winners"] += outside
            if loser_rank == 0 or winner_rank == 0:
                candidate_rank = winner_rank if loser_rank == 0 else loser_rank
                candidate = variants[candidate_rank]
                result = {"rank": candidate_rank,
                          "outside_original_16": candidate["baseline_visits"] == 0,
                          "candidate_better": loser_rank == 0,
                          "reasons": candidate["reasons"]}
                candidate_winners.append(result)
                group = "outside" if result["outside_original_16"] else "inside"
                confirmed_groups[group].append((case_id, result))
                for reason in result["reasons"]:
                    confirmed_groups[f"reason:{reason}"].append((case_id, result))
        # Quality is conditional on both layouts completing under the same
        # seed; it is deliberately separate from completion-winner labels.
        for left_rank, right_rank in itertools.combinations(range(len(variants)), 2):
            left_q = variants[left_rank]["search_evidence"]["c16-n128"]
            right_q = variants[right_rank]["search_evidence"]["c16-n128"]
            if (left_q["visits"] == 0 or right_q["visits"] == 0
                    or left_q["q"] == right_q["q"]):
                continue
            higher, lower = (left_rank, right_rank) if left_q["q"] > right_q["q"] \
                else (right_rank, left_rank)
            for seed in PRIMARY_SEEDS + REPLICATION_SEEDS:
                a, b = trial[case_id][higher][seed], trial[case_id][lower][seed]
                if not a["completed"] or not b["completed"]:
                    continue
                area = a["footprint_area"] - b["footprint_area"]
                transport = a["transport_cells"] - b["transport_cells"]
                quality["paired_success_seed_samples"] += 1
                quality["cases"].add(case_id)
                quality["q_higher_smaller_area"] += area < 0
                quality["q_higher_larger_area"] += area > 0
                quality["equal_area"] += area == 0
                quality["area_differences"].append(area)
                quality["transport_differences"].append(transport)
        cases.append({"stream": source["stream"], "case_id": case_id,
                      "primary_clear_pairs": repeat["primary_clear_pairs"],
                      "confirmed_pairs": len(confirmed),
                      "candidate_vs_baseline_confirmed": candidate_winners})
    groups = {name: {"comparisons": len(rows),
                     "cases": len({case_id for case_id, _ in rows}),
                     "candidate_better": sum(row["candidate_better"] for _, row in rows),
                     "candidate_worse": sum(not row["candidate_better"] for _, row in rows),
                     "cases_candidate_better": len({case_id for case_id, row in rows
                                                   if row["candidate_better"]})}
              for name, rows in sorted(confirmed_groups.items())}
    q_compact = {name: dict(metric, cases=len(metric["cases"]))
                 for name, metric in q_rank.items()}
    quality_compact = {key: value for key, value in quality.items()
                       if key not in ("cases", "area_differences",
                                      "transport_differences")}
    quality_compact["cases"] = len(quality["cases"])
    quality_compact["mean_area_difference_q_high_minus_low"] = statistics.mean(
        quality["area_differences"]) if quality["area_differences"] else None
    quality_compact["mean_transport_difference_q_high_minus_low"] = statistics.mean(
        quality["transport_differences"]) if quality["transport_differences"] else None
    output = {"schema": "aic-hierarchical-continuation-replication-analysis-v1",
              "primary_summary_sha256": sha(root / "continuation-summary.json"),
              "replication_summary_sha256": sha(root / "replication-summary.json"),
              "cases": sorted(cases, key=lambda case: case["case_id"]),
              "groups": groups, "confirmed_q_ranking": q_compact,
              "conditional_quality_q_ranking": quality_compact}
    write(root / "replication-analysis.json", output)
    print({"confirmed_pairs": replication["confirmed_pairs"],
           "groups": groups, "confirmed_q_ranking": q_compact,
           "conditional_quality_q_ranking": quality_compact})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    args = parser.parse_args()
    analyze(args.output)


if __name__ == "__main__":
    main()
