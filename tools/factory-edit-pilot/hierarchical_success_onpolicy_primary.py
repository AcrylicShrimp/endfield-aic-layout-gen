"""Close the predeclared on-policy primary gate without optional seed stress."""

import argparse
from pathlib import Path
import statistics

from hierarchical_target_learner import STREAMS
from ppo_harness_state import read, sha, write


def summarize(root: Path) -> dict:
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    if (progress["status"] != "complete"
            or progress["protocol_sha256"] != sha(root / "protocol.json")
            or len(progress["records"]) != protocol["planned_records"]):
        raise ValueError("hierarchical-success-onpolicy: primary run incomplete")

    by = {(row["stream"], row["generation"], row["arm"]): row
          for row in progress["records"]}
    final_completion = [by[(stream, 3, "success-on")]["completed"]
                        - by[(stream, 3, "current")]["completed"]
                        for stream in STREAMS]
    final_retention = [by[(stream, 3, "current")]["lost_from_start"]
                       - by[(stream, 3, "success-on")]["lost_from_start"]
                       for stream in STREAMS]
    later_completion = [by[(stream, generation, "success-on")]["completed"]
                        - by[(stream, generation, "current")]["completed"]
                        for stream in STREAMS for generation in (2, 3)]
    gates = protocol["numeric_gate"]
    checks = {
        "final_mean_completion_gain": (
            statistics.mean(final_completion)
            >= gates["final_mean_completion_gain"]),
        "final_stream_wins": (
            sum(value > 0 for value in final_completion)
            >= gates["final_stream_wins"]),
        "final_mean_fewer_lost_start_successes": (
            statistics.mean(final_retention)
            >= gates["final_mean_fewer_lost_start_successes"]),
        "final_positive_retention_streams": (
            sum(value > 0 for value in final_retention)
            >= gates["final_positive_retention_streams"]),
        "generation_2_3_mean_completion_gain": (
            statistics.mean(later_completion)
            >= gates["generation_2_3_mean_completion_gain"]),
    }
    return {
        "schema": "aic-hierarchical-success-onpolicy-primary-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "streams": list(STREAMS),
        "baseline_completed": progress["baseline"]["completed"],
        "final_completion_differences": final_completion,
        "final_retention_differences": final_retention,
        "later_completion_differences": later_completion,
        "primary_checks": checks,
        "passes_primary_gate": all(checks.values()),
        "stress_required_for_final_gate": all(checks.values()),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = summarize(args.output)
    write(args.output / "primary-summary.json", result)
    print(f"passes_primary_gate={result['passes_primary_gate']}")
