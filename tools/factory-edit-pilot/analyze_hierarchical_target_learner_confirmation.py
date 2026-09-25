"""Apply the declared retention gate to completed target-learner streams."""

import argparse
import json
from pathlib import Path
import statistics

from ppo_harness_state import read, sha, write


def require(ok, message):
    if not ok:
        raise ValueError(f"hierarchical-target-confirmation-analysis: {message}")


def changes(before, after):
    require(len(before) == len(after) == 64, "diagnostic mask length differs")
    return {"lost": sum(a == "1" and b == "0" for a, b in zip(before, after)),
            "gained": sum(a == "0" and b == "1" for a, b in zip(before, after))}


def analyze(root):
    directory = root / "learner/confirmation"
    protocol = read(directory / "protocol.json")
    progress = read(directory / "progress.json")
    observer = read(directory / "wandb-observer.json")
    require(progress["status"] == "complete" and len(progress["records"]) == 18
            and progress["protocol_sha256"] == sha(directory / "protocol.json")
            and protocol["phase"] == "confirmation"
            and observer["protocol_sha256"] == sha(directory / "protocol.json")
            and observer["status"] == "finished"
            and observer["remote_confirmed_step"] >= 18,
            "confirmation or remote tracking is incomplete")
    by_name = {row["name"]: row for row in progress["records"]}
    require(len(by_name) == 18, "duplicate training row")
    streams = []
    for stream in ("s1", "s2", "s3"):
        arms = {}
        for arm in ("current", "contrast"):
            rows = [by_name[f"{stream}-g{generation}-{arm}"]
                    for generation in range(3)]
            masks = [row["case_success"] for row in rows]
            arms[arm] = {
                "completed": [row["completed"] for row in rows],
                "lost_from_start": [row["lost_from_start"] for row in rows],
                "gained_from_start": [row["gained_from_start"] for row in rows],
                "easy_completed": [row["easy_completed"] for row in rows],
                "hard_completed": [row["hard_completed"] for row in rows],
                "transitions": [changes(a, b) for a, b in zip(masks, masks[1:])],
            }
        current = by_name[f"{stream}-g2-current"]
        contrast = by_name[f"{stream}-g2-contrast"]
        masks = (current["case_success"], contrast["case_success"])
        streams.append({"stream": stream, "arms": arms,
            "final_retention_difference": current["lost_from_start"]
                                          - contrast["lost_from_start"],
            "final_completion_difference": contrast["completed"] - current["completed"],
            "final_both_solved": sum(a == b == "1" for a, b in zip(*masks)),
            "final_current_only": sum(a == "1" and b == "0" for a, b in zip(*masks)),
            "final_contrast_only": sum(a == "0" and b == "1" for a, b in zip(*masks)),
        })
    retention = [row["final_retention_difference"] for row in streams]
    completions = [row["final_completion_difference"] for row in streams]
    gate = {"retention_differences": retention,
            "mean_retention_difference": statistics.mean(retention),
            "streams_with_better_retention": sum(value > 0 for value in retention),
            "completion_differences": completions,
            "mean_completion_difference": statistics.mean(completions),
            "passed": (statistics.mean(retention) >= 3
                       and sum(value > 0 for value in retention) >= 2
                       and statistics.mean(completions) >= -2)}
    output = {"schema": "aic-hierarchical-target-learner-confirmation-analysis-v1",
        "protocol_sha256": sha(directory / "protocol.json"),
        "progress_sha256": sha(directory / "progress.json"),
        "wandb_status_sha256": sha(directory / "wandb-observer.json"),
        "baseline_completed": progress["baseline"]["completed"],
        "streams": streams, "retention_gate": gate}
    write(root / "learner-confirmation-analysis.json", output)
    print(json.dumps({"retention_gate": gate, "streams": streams}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    args = parser.parse_args()
    analyze(args.output)
