"""Summarize a completed, paired outer-policy update-budget probe."""

import argparse
from collections import Counter
import json
from pathlib import Path

import numpy as np
from safetensors.numpy import load_file

from ppo_harness_state import read, sha, write


STREAMS = ("s1", "s2", "s3")
BUDGETS = (25, 50, 100, 200)


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-update-budget.summary: {message}")


def movement(start, end):
    source = load_file(start)
    result = load_file(end)
    require(source.keys() == result.keys(), "outer parameter keys differ")
    squared_delta = 0.0
    squared_source = 0.0
    for name in source:
        before = source[name].astype(np.float64, copy=False)
        after = result[name].astype(np.float64, copy=False)
        require(before.shape == after.shape, f"outer parameter shape differs: {name}")
        squared_delta += float(np.sum((after - before) ** 2))
        squared_source += float(np.sum(before ** 2))
    return {"l2_delta": squared_delta ** 0.5,
            "relative_l2_delta": (squared_delta / squared_source) ** 0.5}


def eval_summary(path, expected_sha, case_order):
    require(sha(path) == expected_sha, f"evaluation hash changed: {path}")
    report = read(path)
    require(report["status"] == "complete" and report["partition"] == "training"
            and len(report["cases"]) == len(case_order), f"invalid evaluation: {path}")
    cases = {case["id"]: case["mcts"] for case in report["cases"]}
    require(set(cases) == set(case_order), f"evaluation case identities changed: {path}")
    return {
        "termination": dict(sorted(Counter(cases[case_id]["termination"]
                                            for case_id in case_order).items())),
        "search_seconds": sum(cases[case_id]["elapsed_seconds"] for case_id in case_order),
        "raw_bytes": path.stat().st_size,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True,
                        help="experiment artifact root")
    args = parser.parse_args()
    root = args.output
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    baseline = read(root / "baseline.json")
    observer = read(root / "wandb-observer.json")
    require(progress["status"] == "complete" and len(progress["records"]) == 12,
            "probe is incomplete")
    require(progress["protocol_sha256"] == sha(root / "protocol.json")
            and progress["baseline_sha256"] == sha(root / "baseline.json"),
            "protocol or baseline hash changed")
    require(observer["status"] == "finished" and observer["remote_confirmed_step"] >= 12,
            "W&B result records are not acknowledged")
    case_order = baseline["case_order"]
    start = [digit == "1" for digit in baseline["case_success"]]
    require(len(case_order) == 64 and len(start) == 64 and sum(start) == baseline["completed"],
            "baseline case mask differs")
    baseline_stats = eval_summary(root / "baseline-raw.json",
                                  baseline["raw_report_sha256"], case_order)
    rows = {}
    for record in progress["records"]:
        stream, budget = record["stream"], record["epochs"]
        require(stream in STREAMS and budget in BUDGETS
                and (stream, budget) not in rows, "duplicate or unknown arm")
        path = root / "evaluations" / stream / f"{budget}.json"
        stats = eval_summary(path, record["evaluation_report_sha256"], case_order)
        mask = [digit == "1" for digit in record["case_success"]]
        require(len(mask) == 64 and sum(mask) == record["completed"],
                f"case mask differs: {stream}-{budget}")
        require(sum(now and not before for before, now in zip(start, mask))
                == record["gained_vs_start"]
                and sum(before and not now for before, now in zip(start, mask))
                == record["lost_vs_start"], f"paired counts differ: {stream}-{budget}")
        trained = read(root / "training" / stream / str(budget) / "report.json")
        require(sha(root / "training" / stream / str(budget) / "report.json")
                == record["training_report_sha256"], f"training report changed: {stream}-{budget}")
        rows[stream, budget] = {
            "completed": record["completed"],
            "gained_vs_start": record["gained_vs_start"],
            "lost_vs_start": record["lost_vs_start"],
            "easy_completed": record["easy_completed"],
            "updates": record["updates"],
            "training_seconds": record["training_seconds"],
            "initial_outer_loss": record["initial_outer_loss"],
            "final_outer_loss": record["final_outer_loss"],
            "model_movement": movement(root / "inputs" / "initial-outer" / "state.safetensors",
                                       root / "training" / stream / str(budget)
                                       / "outer" / "state.safetensors"),
            "evaluation": stats,
            "case_success": record["case_success"],
            "outer_checkpoint_sha256": record["outer_checkpoint_sha256"],
            "collection_sha256": record["collection_sha256"],
        }
        require(trained["lower_checkpoint_sha256"] == protocol["initial_lower_sha256"],
                f"lower weights changed: {stream}-{budget}")
    require(len(rows) == 12, "missing arm")
    contrasts = {}
    for stream in STREAMS:
        control = rows[stream, 200]
        comparisons = {}
        for budget in (25, 50, 100):
            candidate = rows[stream, budget]
            control_mask = [x == "1" for x in control["case_success"]]
            candidate_mask = [x == "1" for x in candidate["case_success"]]
            comparisons[str(budget)] = {
                "completed_delta_vs_200": candidate["completed"] - control["completed"],
                "lost_from_start_delta_vs_200": (candidate["lost_vs_start"]
                                                 - control["lost_vs_start"]),
                "candidate_only": sum(now and not old for now, old
                                      in zip(candidate_mask, control_mask)),
                "control_only": sum(old and not now for now, old
                                    in zip(candidate_mask, control_mask)),
            }
        contrasts[stream] = comparisons
    retention_advantages = [rows[stream, 200]["lost_vs_start"]
                            - rows[stream, 100]["lost_vs_start"] for stream in STREAMS]
    mean_completion_delta = sum(rows[stream, 100]["completed"]
                                - rows[stream, 200]["completed"] for stream in STREAMS) / 3
    gate = {
        "retention_advantage_by_stream": dict(zip(STREAMS, retention_advantages)),
        "mean_retention_advantage": sum(retention_advantages) / 3,
        "positive_retention_streams": sum(value > 0 for value in retention_advantages),
        "mean_completed_delta_100_vs_200": mean_completion_delta,
        "overshoot_signal": (sum(retention_advantages) / 3 >= 3
                             and sum(value > 0 for value in retention_advantages) >= 2
                             and mean_completion_delta >= -2),
    }
    collection_stats = {}
    for stream in STREAMS:
        path = root / "collections" / f"{stream}.json"
        collection = read(path)
        require(collection["status"] == "complete" and len(collection["cases"]) == 32
                and sha(path) == rows[stream, 25]["collection_sha256"],
                f"collection changed: {stream}")
        collection_stats[stream] = {
            "search_seconds": sum(case["mcts"]["elapsed_seconds"]
                                  for case in collection["cases"]),
            "raw_bytes": path.stat().st_size,
        }
    resource_summary = {
        "integrated_wall_seconds": (root / "wandb-observer.json").stat().st_mtime
                                   - (root / "protocol.json").stat().st_mtime,
        "artifact_bytes_before_cleanup": sum(path.stat().st_size for path in root.rglob("*")
                                             if path.is_file()),
        "collection": collection_stats,
    }
    output = {
        "schema": "aic-hierarchical-update-budget-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "baseline_sha256": sha(root / "baseline.json"),
        "observer_run_url": observer["url"],
        "observer_remote_confirmed_step": observer["remote_confirmed_step"],
        "baseline": {"completed": baseline["completed"],
                     "easy_completed": baseline["easy_completed"],
                     "case_success": baseline["case_success"],
                     "evaluation": baseline_stats},
        "rows": {stream: {str(budget): rows[stream, budget] for budget in BUDGETS}
                 for stream in STREAMS},
        "paired_vs_200": contrasts,
        "predeclared_gate": gate,
        "resources": resource_summary,
    }
    write(root / "summary.json", output)
    print(json.dumps(gate, indent=2))


if __name__ == "__main__":
    main()
