"""Summarize how paired target learners change their later on-policy data."""

import argparse
import hashlib
import json
from pathlib import Path

from hierarchical_candidate_width import action_key
from ppo_harness_state import read, sha, write


def require(ok, message):
    if not ok:
        raise ValueError(f"hierarchical-target-feedback: {message}")


def analyze(root):
    learner = root / "learner"
    directory = learner / "confirmation"
    protocol = read(directory / "protocol.json")
    progress = read(directory / "progress.json")
    require(progress["status"] == "complete" and len(progress["records"]) == 18
            and progress["protocol_sha256"] == sha(directory / "protocol.json")
            and protocol["phase"] == "confirmation", "confirmation is incomplete")
    rows = {}
    for record in progress["records"]:
        stream, generation, arm = record["stream"], record["generation"] - 1, record["arm"]
        source_arm = "current" if generation == 0 else arm
        source = directory / "collections" / f"{stream}-g{generation}-{source_arm}-source.json"
        require(sha(source) == record["source_collection_sha256"],
                f"collection changed: {record['name']}")
        report = read(source)
        panel = read(learner / "panels" / f"retention-{stream}-g{generation}.json")
        order = [case["id"] for case in panel["cases"]]
        cases = {case["id"]: case for case in report["cases"]}
        require(report["status"] == "complete" and report["panel_sha256"] == sha(
                learner / "panels" / f"retention-{stream}-g{generation}.json")
                and len(order) == 32 and set(cases) == set(order),
                f"source panel differs: {record['name']}")
        selected = []
        succeeded = []
        for case_id in order:
            case = cases[case_id]
            first = case["mcts"]["root_records"][0]
            action = first["actions"][first["selected_action"]]
            selected.append(hashlib.sha256(action_key(action).encode()).hexdigest())
            succeeded.append(bool(case["mcts"]["fully_validated"]))
        rows[(stream, generation, arm)] = {
            "stream": stream, "generation": generation + 1, "arm": arm,
            "source_collection_sha256": sha(source),
            "source_outer_checkpoint_sha256": report["outer_checkpoint_sha256"],
            "case_ids": order, "source_search_completed": sum(succeeded),
            "source_search_case_success": "".join("1" if x else "0" for x in succeeded),
            "selected_first_action_sha256": selected,
        }
    pairs = []
    for stream in ("s1", "s2", "s3"):
        for generation in range(3):
            current = rows[(stream, generation, "current")]
            contrast = rows[(stream, generation, "contrast")]
            require(current["case_ids"] == contrast["case_ids"],
                    f"pair panel differs: {stream}/{generation}")
            current_mask = current["source_search_case_success"]
            contrast_mask = contrast["source_search_case_success"]
            same_source = (current["source_collection_sha256"]
                           == contrast["source_collection_sha256"])
            require(generation != 0 or same_source,
                    f"shared first-generation source differs: {stream}")
            pairs.append({"stream": stream, "generation": generation + 1,
                "same_source_report": same_source,
                "same_selected_first_actions": sum(a == b for a, b in zip(
                    current["selected_first_action_sha256"],
                    contrast["selected_first_action_sha256"])),
                "current_source_completed": current["source_search_completed"],
                "contrast_source_completed": contrast["source_search_completed"],
                "contrast_only_source_successes": sum(a == "0" and b == "1"
                    for a, b in zip(current_mask, contrast_mask)),
                "current_only_source_successes": sum(a == "1" and b == "0"
                    for a, b in zip(current_mask, contrast_mask))})
    output = {"schema": "aic-hierarchical-target-feedback-analysis-v1",
        "confirmation_protocol_sha256": sha(directory / "protocol.json"),
        "confirmation_progress_sha256": sha(directory / "progress.json"),
        "rows": list(rows.values()), "pairs": pairs}
    write(root / "learner-feedback-analysis.json", output)
    print(json.dumps(pairs, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    args = parser.parse_args()
    analyze(args.output)
