"""Decompose completed training wall time without double-counting nested clocks."""

import argparse
import json
from pathlib import Path

from plots import read_rows
from evidence import write


def costs(directory):
    metadata = json.loads((directory/"metadata.json").read_text())
    if metadata["status"] != "complete":
        raise ValueError(f"costs.incomplete: {directory}")
    updates = read_rows(directory/"updates.jsonl")
    if not updates or any("seconds" not in row for row in updates):
        raise ValueError(f"costs.missing_update_timing: {directory}/updates.jsonl")
    env = metadata["environment_seconds"]
    parts = {"actor_inference": metadata["inference_seconds"],
             "observation": env["observation"], "candidates": env["candidates"],
             "validation": env["validation"], "other_environment_step": env["step"]-env["validation"],
             "actor_snapshot": metadata.get("snapshot_seconds", 0)}
    if metadata.get("actor_compile_setup_seconds", 0):
        parts["actor_compile_setup"] = metadata["actor_compile_setup_seconds"]
    if env.get("array_export", 0):
        parts["native_array_export"] = env["array_export"]
    update_parts = {f"ppo_{key}": sum(row["seconds"][key] for row in updates)
                    for key in ("batch", "forward_loss", "backward", "optimizer")}
    parts["other_ppo_update"] = metadata["update_seconds"]-sum(update_parts.values())
    parts.update(update_parts)
    parts["other_training"] = metadata["training_seconds"]-sum(parts.values())
    if min(parts.values()) < -1e-6:
        raise ValueError(f"costs.inconsistent_nested_clocks: {directory}: {parts}")
    parts = {key: max(0, value) for key, value in parts.items()}
    return {"directory": str(directory.resolve()), "commands": metadata["completed_commands"],
            "segment_commands": metadata.get("segment_commands", metadata["completed_commands"]),
            "training_seconds": metadata["training_seconds"], "evaluation_seconds": metadata["evaluation_seconds"],
            "parts": [{"stage": key, "seconds": value, "percent": value/metadata["training_seconds"]*100}
                      for key, value in sorted(parts.items(), key=lambda pair: -pair[1])],
            "peak_rss_bytes": metadata["peak_rss_bytes"],
            "peak_cuda_allocated_bytes": metadata.get("peak_cuda_allocated_bytes", 0),
            "environment_counters": metadata["environment_counters"],
            "preparation_modes": sorted({row.get("preparation", {}).get("mode", "minibatch") for row in updates}),
            "maximum_prepared_estimate_bytes": max(row.get("preparation", {}).get("estimated_rollout_bytes", 0) for row in updates)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", nargs="+", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f"Refusing to overwrite {args.output}")
    args.output.mkdir(parents=True)
    result = {"schema_version": 1, "timing_scope": "current_invocation_only",
              "comparison": "Independent recorded runs; equal seed does not imply equal trajectories",
              "runs": [costs(path) for path in args.runs]}
    write(args.output/"costs.json", result)
    print(json.dumps(result, indent=2))
