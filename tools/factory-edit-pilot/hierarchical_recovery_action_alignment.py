"""Check search-arm root values against earlier replicated action outcomes."""

import argparse
import math
from pathlib import Path
import statistics

from hierarchical_recovery_search import ARMS, SEEDS, STREAMS, action_digest, name
from ppo_harness_state import read, sha, write


ROOT = Path("docs/benchmarks/hierarchical-target-causal")
REPLICATION = Path("docs/reports/2026-09-25-hierarchical-continuation-replication-summary.json")


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-recovery-action-alignment: {message}")


def analyze(root):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    require(progress["status"] == "complete"
            and progress["protocol_sha256"] == sha(root / "protocol.json")
            and len(progress["records"]) == protocol["planned_cells"],
            "search screen incomplete")
    replication = read(REPLICATION)
    confirmed = {row["case_id"]: row["confirmed_pairs"]
                 for row in replication["cases"] if row["confirmed_pairs"]}
    candidates = {}
    for stream in STREAMS:
        source = read(ROOT / f"continuation-actions-{stream}.json")
        for case in source["cases"]:
            require(case["case_id"] not in candidates, "duplicate candidate case")
            candidates[case["case_id"]] = [row["action_sha256"]
                                            for row in case["candidates"]]
    require(set(confirmed) <= set(candidates), "confirmed action witness missing")
    results = {}
    for arm in ARMS:
        for seed in SEEDS:
            rows = []
            total_roots = visible_roots = 0
            early_roots = early_visible_roots = 0
            for stream in STREAMS:
                path = root / "raw" / stream / f"{name(arm, seed)}.json"
                record = next(row for row in progress["records"]
                              if (row["stream"], row["arm"], row["seed"])
                              == (stream, arm, seed))
                require(sha(path) == record["report_sha256"], "search report changed")
                report = read(path)
                for case in report["cases"]:
                    roots = case["mcts"]["root_records"]
                    total_roots += len(roots)
                    early_roots += min(4, len(roots))
                    for index, record in enumerate(roots):
                        visible = any(action["visits"] > 0 and action["q"] > 1.0
                                      for action in record["actions"])
                        visible_roots += visible
                        early_visible_roots += visible if index < 4 else 0
                    if case["id"] not in confirmed:
                        continue
                    root_record = case["mcts"]["root_records"][0]
                    actions = {action_digest(action): action
                               for action in root_record["actions"]}
                    require(len(actions) == len(root_record["actions"]),
                            "duplicate root action")
                    comparisons = []
                    for winner_rank, loser_rank in confirmed[case["id"]]:
                        winner_digest = candidates[case["id"]][winner_rank]
                        loser_digest = candidates[case["id"]][loser_rank]
                        require(winner_digest in actions and loser_digest in actions,
                                "action missing from full root witness")
                        winner = actions[winner_digest]
                        loser = actions[loser_digest]
                        wp, lp = winner["policy_target"], loser["policy_target"]
                        require(wp > 0 and lp > 0, "nonpositive target probability")
                        # Binary logistic loss conditional on this tested pair.
                        difference = math.log(lp) - math.log(wp)
                        pair_loss = max(difference, 0) + math.log1p(
                            math.exp(-abs(difference)))
                        q_sign = None
                        if winner["visits"] > 0 and loser["visits"] > 0:
                            q_sign = (winner["q"] > loser["q"])
                            if winner["q"] == loser["q"]:
                                q_sign = "tie"
                        comparisons.append((pair_loss, q_sign))
                    rows.append({"case_id": case["id"], "pairs": len(comparisons),
                                 "case_mean_pair_loss": statistics.mean(
                                     item[0] for item in comparisons),
                                 "q_winner": sum(item[1] is True for item in comparisons),
                                 "q_loser": sum(item[1] is False for item in comparisons),
                                 "q_tie": sum(item[1] == "tie" for item in comparisons),
                                 "both_visited": sum(item[1] is not None
                                                     for item in comparisons)})
            require(len(rows) == len(confirmed), "confirmed case count differs")
            results[name(arm, seed)] = {
                "case_mean_pair_loss": statistics.mean(
                    row["case_mean_pair_loss"] for row in rows),
                "confirmed_cases": len(rows),
                "confirmed_pairs": sum(row["pairs"] for row in rows),
                "both_visited_pairs": sum(row["both_visited"] for row in rows),
                "q_winner": sum(row["q_winner"] for row in rows),
                "q_loser": sum(row["q_loser"] for row in rows),
                "q_tie": sum(row["q_tie"] for row in rows),
                "terminal_value_visible_roots": visible_roots,
                "total_roots": total_roots,
                "first_four_terminal_value_visible_roots": early_visible_roots,
                "first_four_roots": early_roots,
                "case_rows": rows,
            }
    output = {
        "schema": "aic-hierarchical-recovery-action-alignment-v1",
        "runner_sha256": sha(Path(__file__)),
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "replication_summary_sha256": sha(REPLICATION),
        "action_manifest_sha256": {
            stream: sha(ROOT / f"continuation-actions-{stream}.json")
            for stream in STREAMS},
        "results": results,
    }
    write(root / "action-alignment.json", output)
    for key, row in results.items():
        print(f"{key}: loss={row['case_mean_pair_loss']:.3f} "
              f"q={row['q_winner']}/{row['q_loser']}/{row['q_tie']} "
              f"visited={row['both_visited_pairs']}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    analyze(args.output)


if __name__ == "__main__":
    main()
