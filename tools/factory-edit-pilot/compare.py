"""Plot outcome/reward and completed wall time for a matched CPU/CUDA run pair."""

import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

from plots import read_rows
from evidence import write


def compare(directories, output):
    runs = [json.loads((path/"metadata.json").read_text()) for path in directories]
    baseline = {k: v for k, v in runs[0]["config"].items() if k != "device"}
    for path, run in zip(directories, runs):
        if run["status"] != "complete":
            raise ValueError(f"Incomplete run: {path}")
        if {k: v for k, v in run["config"].items() if k != "device"} != baseline:
            raise ValueError(f"Run budgets/configuration differ beyond learner device: {path}")
        if any(run[key] != runs[0][key] for key in ("case", "parameters", "completed_commands", "optimizer_steps")):
            raise ValueError(f"Run problem/model/exposure mismatch: {path}")
        if any(run["provenance"][key] != runs[0]["provenance"][key]
               for key in ("python", "torch", "numpy", "platform", "machine")):
            raise ValueError(f"Run host/runtime mismatch: {path}")
        previous = runs[0]["provenance"]["files_sha256"]
        current = run["provenance"]["files_sha256"]
        if any(previous[key] != current[key] for key in previous.keys() & current.keys()):
            raise ValueError(f"Common source/catalog hashes differ: {path}")
    output.mkdir(parents=True, exist_ok=True)
    fig, axes = plt.subplots(1, 3, figsize=(16, 4.8), constrained_layout=True)
    colors = ["#2368a0", "#d27328"]
    summary = []
    timing = []
    for directory, run, color in zip(directories, runs, colors):
        label = run["device"].upper()+" learner"
        evaluations = read_rows(directory/"evaluation-summary.jsonl")
        episodes = read_rows(directory/"episodes.jsonl")
        x = [row["commands"] for row in evaluations]
        axes[0].plot(x, [row["mean_return"]*100 for row in evaluations], "o-", color=color, label=label)
        ex = [row["commands"] for row in episodes]
        returns = np.array([row["return"] for row in episodes])
        window = run["config"]["reward_smoothing_episodes"]
        axes[1].scatter(ex, returns, s=5, alpha=.13, color=color)
        axes[1].plot(ex, [returns[max(0, i-window+1):i+1].mean() for i in range(len(returns))], color=color, label=label)
        env = run["environment_seconds"]
        pieces = [run["update_seconds"], run["inference_seconds"]+run.get("snapshot_seconds", 0),
                  env["candidates"]+env["observation"], env["validation"], env["step"]-env["validation"]]
        pieces.append(run["training_seconds"]-sum(pieces))
        if min(pieces) < -1e-6:
            raise ValueError(f"Inconsistent nested timing totals: {directory}")
        timing.append(pieces)
        summary.append({"directory": str(directory), "device": run["device"],
                        "training_seconds": run["training_seconds"], "evaluation_seconds": run["evaluation_seconds"],
                        "update_seconds": run["update_seconds"], "environment_seconds": env,
                        "final_evaluation": evaluations[-1], "positive_training_episodes": int((returns > 0).sum()),
                        "training_champion_score": run["training_champion_score"]})
    axes[0].set(title="Frozen-policy outcome", xlabel="Training commands", ylabel="Mean returned area reduction (%)")
    axes[1].set(title="Training reward", xlabel="Training commands", ylabel="Episode return / up to 32-episode mean")
    for axis in axes[:2]:
        axis.grid(alpha=.2)
        axis.legend(fontsize=9)
    labels = [run["device"].upper() for run in runs]
    names = ["PPO update incl. preparation", "CPU actor + snapshot", "Candidates + observation", "Validation", "Other environment step", "Other recorded training"]
    bottom = np.zeros(len(runs))
    for i, name in enumerate(names):
        heights = [parts[i] for parts in timing]
        axes[2].bar(labels, heights, bottom=bottom, label=name, color=plt.get_cmap("tab20")(i*2))
        bottom += heights
    for i, total in enumerate(bottom):
        axes[2].text(i, total+2, f"{total:.1f} s", ha="center")
    axes[2].set(title="Completed training wall time", ylabel="Seconds (evaluation reported separately)", ylim=(0, max(bottom)*1.17))
    axes[2].legend(fontsize=7, loc="upper center", bbox_to_anchor=(.5, -.08))
    fig.suptitle(f"Same server, fixed factory, {runs[0]['parameters']:,} parameters, {runs[0]['completed_commands']:,} commands\n"
                 "CPU actor in both runs; one training seed per device; float32 without TF32", fontsize=12)
    fig.savefig(output/"cpu-cuda-comparison.png", dpi=170)
    fig.savefig(output/"cpu-cuda-comparison.svg")
    plt.close(fig)
    write(output/"comparison.json", {"schema_version": 1, "runs": summary})
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runs", nargs=2, type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(compare(args.runs, args.output), indent=2))
