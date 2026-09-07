"""Compare every declared two-case condition with total and per-case exposure kept explicit."""

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


def compare(protocol_path, run_paths, output):
    if output.exists():
        raise ValueError(f"study_compare.output_exists: {output}: choose a fresh directory")
    protocol = json.loads(protocol_path.read_text())
    runs, hashes, initializations, signatures = {}, {}, {}, []
    for path in run_paths:
        metadata = json.loads((path / "metadata.json").read_text())
        audit = json.loads((path / "study-audit.json").read_text())
        identity = (metadata["condition"], metadata["config"]["seed"])
        if identity in runs:
            raise ValueError(f"study_compare.duplicate: {path}: repeated condition/seed {identity}")
        digest = hashlib.sha256((path / "metadata.json").read_bytes()).hexdigest()
        if audit["status"] != "passed" or metadata["status"] != "complete" or audit["metadata_sha256"] != digest:
            raise ValueError(f"study_compare.audit: {path}: completed metadata is not covered by the passing audit")
        condition, seed = identity
        if condition not in protocol["conditions"] or seed not in protocol["training_seeds"]:
            raise ValueError(f"study_compare.protocol: {path}: undeclared condition or seed")
        initializations.setdefault(seed, set()).add(audit["initial_policy_sha256"])
        config = {key: value for key, value in metadata["config"].items() if key not in ("seed", "commands", "checkpoints")}
        sources = metadata["provenance"]
        signatures.append({"config": config, "parameters": metadata["parameters"], "dimensions": metadata["dimensions"],
                           "sources": {key: {field: source[field] for field in ("files_sha256", "external_files_sha256", "native",
                                                                              "python", "torch", "numpy", "platform", "machine")}
                                       for key, source in sources.items()}})
        runs[identity] = {"directory": str(path.resolve()), "metadata": metadata, "audit": audit,
                          "evaluations": rows(path / "evaluations.jsonl")}
        hashes[str(path.resolve())] = {name: hashlib.sha256((path / name).read_bytes()).hexdigest()
                                      for name in ("metadata.json", "study-audit.json", "evaluations.jsonl", "episodes.jsonl")}
    expected = {(condition, seed) for condition in protocol["conditions"] for seed in protocol["training_seeds"]}
    if set(runs) != expected or any(len(values) != 1 for values in initializations.values()):
        raise ValueError("study_compare.matrix: missing declared runs or unequal initial models within a training seed")
    if any(signature != signatures[0] for signature in signatures[1:]):
        raise ValueError("study_compare.controls: model/configuration/source/data/runtime identity differs between conditions")
    summaries = []
    for condition, spec in protocol["conditions"].items():
        for count in protocol["checkpoints"]:
            if count > spec["commands"]:
                continue
            for case in protocol["cases"]:
                selected = []
                for seed in protocol["training_seeds"]:
                    found = [row for row in runs[(condition, seed)]["evaluations"]
                             if row["case"] == case and row["commands"] == count]
                    if len(found) != 1:
                        raise ValueError(f"study_compare.evaluation: expected one {condition}/{seed}/{case}/{count} summary")
                    expected_exposure = count // len(spec["schedule"]) if case in spec["schedule"] else 0
                    if found[0]["case_commands"] != expected_exposure:
                        raise ValueError(f"study_compare.exposure: {condition}/{seed}/{case}/{count}: wrong case exposure")
                    selected.append({"seed": seed, **found[0]})
                fields = ("mean_return", "median_return", "improvement_frequency", "multiple_improvement_frequency")
                summaries.append({"condition": condition, "case": case, "commands": count,
                                  "case_commands": selected[0]["case_commands"],
                                  "across_training_seeds": {field: {"mean": float(np.mean([r[field] for r in selected])),
                                                                    "minimum": min(r[field] for r in selected),
                                                                    "maximum": max(r[field] for r in selected)} for field in fields},
                                  "seeds": selected})
    solo_allowances = {spec["commands"] for spec in protocol["conditions"].values() if len(spec["schedule"]) == 1}
    mixed = [(name, spec) for name, spec in protocol["conditions"].items() if len(spec["schedule"]) > 1]
    comparisons = []
    for condition, spec in mixed:
        for case in spec["schedule"]:
            solo = [name for name, candidate in protocol["conditions"].items() if candidate["schedule"] == [case]]
            if len(solo) != 1 or len(solo_allowances) != 1:
                raise ValueError("study_compare.design: expected one solo control per case with a common allowance")
            solo_count = protocol["conditions"][solo[0]]["commands"]
            for basis, mixed_count in (("equal_total_commands", solo_count),
                                       ("equal_case_exposure", solo_count * len(spec["schedule"]))):
                if mixed_count > spec["commands"]:
                    continue
                differences = []
                for seed in protocol["training_seeds"]:
                    a = next(row for row in runs[(solo[0], seed)]["evaluations"] if row["case"] == case and row["commands"] == solo_count)
                    b = next(row for row in runs[(condition, seed)]["evaluations"] if row["case"] == case and row["commands"] == mixed_count)
                    differences.append({"seed": seed, "solo_mean_reduction": a["mean_return"], "mixed_mean_reduction": b["mean_return"],
                                        "mixed_minus_solo_percentage_points": 100 * (b["mean_return"] - a["mean_return"])})
                comparisons.append({"case": case, "basis": basis, "solo_commands": solo_count, "mixed_commands": mixed_count,
                                    "mean_difference_percentage_points": float(np.mean([r["mixed_minus_solo_percentage_points"] for r in differences])),
                                    "seeds": differences})
    output.mkdir(parents=True)
    result = {"schema_version": 1, "study": protocol["id"], "training_seeds": protocol["training_seeds"],
              "replication_unit": "Independent trained policy; evaluation seeds only vary within-policy action sampling",
              "controls": "All declared conditions present; equal initial models per seed and common model/configuration/source/data/runtime",
              "scope": "Two declared training cases; no unseen-factory generalization or optimum claim",
              "summaries": summaries, "comparisons": comparisons,
              "run_costs": [{"condition": condition, "seed": seed, "training_seconds": value["metadata"]["training_seconds"],
                             "evaluation_seconds": value["metadata"]["seconds"]["evaluation"],
                             "case_commands": value["metadata"]["case_commands"], "champions": value["metadata"]["champions"]}
                            for (condition, seed), value in runs.items()],
              "protocol_sha256": hashlib.sha256(protocol_path.read_bytes()).hexdigest(), "artifacts_sha256": hashes}
    write(output / "comparison.json", result)
    render(protocol, summaries, output)
    return result


def render(protocol, summaries, output):
    plt.rcParams.update({"font.size": 10, "axes.spines.top": False, "axes.spines.right": False})
    fig, axes = plt.subplots(1, len(protocol["cases"]), figsize=(12, 5), constrained_layout=True, squeeze=False)
    for axis, case in zip(axes[0], protocol["cases"]):
        for condition in protocol["conditions"]:
            selected = [row for row in summaries if row["case"] == case and row["condition"] == condition]
            x = [row["commands"] for row in selected]
            mean = [100 * row["across_training_seeds"]["mean_return"]["mean"] for row in selected]
            lower = [100 * row["across_training_seeds"]["mean_return"]["minimum"] for row in selected]
            upper = [100 * row["across_training_seeds"]["mean_return"]["maximum"] for row in selected]
            line, = axis.plot(x, mean, "o-", label=condition)
            axis.fill_between(x, lower, upper, color=line.get_color(), alpha=.12)
        axis.set(title=f"Case {case}: frozen-policy outcome", xlabel="Total training commands", ylabel="Mean returned area reduction (%)")
        axis.grid(alpha=.2)
        axis.ticklabel_format(axis="x", style="sci", scilimits=(0, 0))
        axis.legend()
    evaluation_rollouts = summaries[0]["seeds"][0]["evaluation_episodes"]
    fig.suptitle(f"Two-case learning | {len(protocol['training_seeds'])} independent training seeds\n"
                 f"Lines: mean across trained policies; bands: minimum to maximum policy mean; {evaluation_rollouts} evaluation rollouts per policy")
    fig.savefig(output / "learning-comparison.png", dpi=170)
    fig.savefig(output / "learning-comparison.svg")
    plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--runs", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = compare(args.protocol.resolve(), [path.resolve() for path in args.runs], args.output.resolve())
    print(json.dumps(result["comparisons"], indent=2))


if __name__ == "__main__":
    main()
