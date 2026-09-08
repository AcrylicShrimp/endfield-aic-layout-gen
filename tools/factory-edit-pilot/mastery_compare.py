"""Compare the complete frozen factorial matrix using independent trained policies."""

import argparse
import hashlib
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from evidence import write
from native_training_comparison import rows


def compare(protocol_path, paths, output):
    if output.exists():
        raise ValueError(f"mastery_compare.output_exists: {output}")
    protocol = json.loads(protocol_path.read_text())
    runs, initial, signatures = {}, {}, []
    for path in paths:
        m = json.loads((path / "metadata.json").read_text())
        audit = json.loads((path / "study-audit.json").read_text())
        key = (m["condition"], m["seed"])
        if (key in runs or m["status"] != "complete" or audit["status"] != "passed"
                or audit["metadata_sha256"] != hashlib.sha256((path / "metadata.json").read_bytes()).hexdigest()):
            raise ValueError(f"mastery_compare.audit: {path}: duplicate, incomplete or unaudited run")
        initial.setdefault(m["seed"], set()).add(m["initial_policy_sha256"])
        signatures.append({"config": {k: v for k, v in m["config"].items() if k != "seed"},
                           "sources": {k: {f: v[f] for f in ("files_sha256", "external_files_sha256", "native", "python", "torch", "numpy", "platform", "machine")}
                                       for k, v in m["provenance"].items()}, "dimensions": m["dimensions"], "parameters": m["parameters"]})
        runs[key] = {"metadata": m, "evaluations": rows(path / "evaluations.jsonl"),
                     "episodes": rows(path / "episodes.jsonl"), "directory": str(path),
                     "hashes": {name: hashlib.sha256((path / name).read_bytes()).hexdigest()
                                for name in ("metadata.json", "study-audit.json", "episodes.jsonl", "evaluations.jsonl")}}
    if (set(runs) != {(c, s) for c in protocol["conditions"] for s in protocol["training_seeds"]}
            or any(len(v) != 1 for v in initial.values()) or any(s != signatures[0] for s in signatures[1:])):
        raise ValueError("mastery_compare.controls: missing run or unequal initialization/configuration/source/runtime")
    summaries = []
    fields = ("mean_return", "median_return", "improvement_frequency", "multiple_improvement_frequency")
    for condition in protocol["conditions"]:
        for count in protocol["checkpoints"]:
            for case in protocol["cases"]:
                selected = [{"seed": seed, **next(row for row in runs[condition, seed]["evaluations"]
                                                  if row["case"] == case and row["commands"] == count)}
                            for seed in protocol["training_seeds"]]
                summaries.append({"condition": condition, "commands": count, "case": case, "seeds": selected,
                                  "across_training_seeds": {field: {"mean": float(np.mean([row[field] for row in selected])),
                                                                    "minimum": min(row[field] for row in selected),
                                                                    "maximum": max(row[field] for row in selected)} for field in fields}})
    contrasts = []
    pairs = [("long-fixed", "short-fixed"), ("long-mastery", "short-mastery"),
             ("long-fixed", "long-mastery"), ("short-fixed", "short-mastery")]
    for before, after in pairs:
        if before not in protocol["conditions"] or after not in protocol["conditions"]:
            continue
        for case in protocol["cases"]:
            values = []
            for seed in protocol["training_seeds"]:
                a, b = [next(row for row in runs[c, seed]["evaluations"] if row["case"] == case and row["commands"] == protocol["commands"])
                        for c in (before, after)]
                values.append({"seed": seed, "difference_percentage_points": 100 * (b["mean_return"] - a["mean_return"])})
            contrasts.append({"before": before, "after": after, "case": case, "seeds": values,
                              "mean_difference_percentage_points": float(np.mean([v["difference_percentage_points"] for v in values]))})
    costs = []
    for (condition, seed), run in runs.items():
        m = run["metadata"]
        starts = json.loads((Path(run["directory"]) / "starts.json").read_text())
        costs.append({"condition": condition, "seed": seed, "training_seconds": m["training_seconds"],
                      "seconds": m["seconds"], "collector_seconds": m["collector_seconds"],
                      "setup_commands": m["setup_commands"], "stage_commands": m["stage_commands"],
                      "champions": m["champions"], "positive_training_episodes": sum(row["return"] > 0 for row in run["episodes"]),
                      "positive_root_training_episodes": sum(row["return"] > 0 and not starts[row["case"]][str(row["remaining"])]
                                                              for row in run["episodes"]),
                      "training_horizon": m["training_config"]["horizon"], "episodes": m["episodes"], "controller": m["controller"],
                      "assessments": [{key: row[key] for key in ("commands", "training_level", "level", "successes", "transition")}
                                      for row in rows(Path(run["directory"]) / "assessments.jsonl")]})
    output.mkdir(parents=True)
    result = {"schema_version": 1, "protocol": protocol, "summaries": summaries, "contrasts": contrasts, "costs": costs,
              "replication_unit": "Independent trained policy; fixed evaluation seeds vary sampling within a policy",
              "scope": "Original roots of two training cases; no unseen-case or optimality claim",
              "artifacts_sha256": {run["directory"]: run["hashes"] for run in runs.values()}}
    if "stage_acceptance" in protocol:
        criterion = protocol["stage_acceptance"]
        checkpoints = protocol["checkpoints"][-criterion["consecutive_final_checkpoints"]:]
        checked = []
        for summary in summaries:
            if summary["commands"] not in checkpoints:
                continue
            for row in summary["seeds"]:
                if row["evaluation_episodes"] != criterion["evaluation_episodes"]:
                    raise ValueError("mastery_compare.stage_acceptance: outcome episode count differs from the declared criterion")
                successes = round(row["improvement_frequency"] * row["evaluation_episodes"])
                checked.append({"condition": summary["condition"], "commands": summary["commands"], "case": summary["case"],
                                "seed": row["seed"], "successes": successes, "passed": successes >= criterion["root_successes"]})
        result["stage_acceptance"] = {"status": "passed" if all(row["passed"] for row in checked) else "not_met",
                                      "criterion": criterion, "checks": checked}
    write(output / "comparison.json", result)
    fig, axes = plt.subplots(1, len(protocol["cases"]), figsize=(12, 5), constrained_layout=True, squeeze=False)
    for axis, case in zip(axes[0], protocol["cases"]):
        for condition in protocol["conditions"]:
            selected = [row for row in summaries if row["case"] == case and row["condition"] == condition]
            x = [row["commands"] for row in selected]
            values = [row["across_training_seeds"]["mean_return"] for row in selected]
            line, = axis.plot(x, [100*v["mean"] for v in values], "o-", label=condition)
            axis.fill_between(x, [100*v["minimum"] for v in values], [100*v["maximum"] for v in values], color=line.get_color(), alpha=.1)
        if "fixed_curriculum" in protocol:
            axis.axvline(protocol["fixed_curriculum"][-1]["until"], color="gray", ls=":", label="Fixed schedule switches to roots")
        axis.set(title=f"Case {case}: original-root evaluation", xlabel="Sampled training edits", ylabel="Mean returned area reduction (%)")
        axis.grid(alpha=.2)
        axis.legend(fontsize=8)
    fig.suptitle("Short episodes and mastery-gated PPO | means and ranges across independent training seeds")
    fig.savefig(output / "learning-comparison.png", dpi=170)
    plt.close(fig)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--runs", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = compare(args.protocol.resolve(), [p.resolve() for p in args.runs], args.output.resolve())
    print(json.dumps({"status": "passed", "contrasts": result["contrasts"],
                      "stage_acceptance": result.get("stage_acceptance")}, indent=2))


if __name__ == "__main__":
    main()
