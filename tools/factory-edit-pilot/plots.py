"""Standalone outcome, reward, and geometry figures from recorded experiment data."""

import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.patches import Rectangle
import numpy as np

from acceptance import Catalog, check, footprint


def read_rows(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()] if Path(path).exists() else []


def render(output, catalog):
    output = Path(output)
    metadata = json.loads((output/"metadata.json").read_text())
    episodes = read_rows(output/"episodes.jsonl")
    evaluations = read_rows(output/"evaluations.jsonl")
    summaries = read_rows(output/"evaluation-summary.jsonl")
    config = metadata["config"]
    plt.rcParams.update({"font.size": 10, "axes.spines.top": False, "axes.spines.right": False})
    fig, axes = plt.subplots(1, 3, figsize=(15.5, 4.8), constrained_layout=True)
    x = [s["commands"] for s in summaries]
    mean = np.array([s["mean_return"] for s in summaries])*100
    std = np.array([s["std_return"] for s in summaries])*100
    axes[0].plot(x, mean, "o-", color="#2368a0", label="Frozen policy: mean")
    axes[0].fill_between(x, mean-std, mean+std, color="#2368a0", alpha=.16, label="Rollout +/-1 SD")
    axes[0].plot(x, [s["max_return"]*100 for s in summaries], "s--", color="#009b77", label="Best of 32 evaluation episodes")
    axes[0].set(title="Factory outcome", ylabel="Final area reduction (%)", xlabel="Training commands")
    axes[0].legend(fontsize=8)
    if episodes:
        ex = np.array([e["commands"] for e in episodes])
        returns = np.array([e["return"] for e in episodes])
        axes[1].scatter(ex, returns, color="#9ca7b3", s=6, alpha=.5, label="Training episode return")
        window = config["reward_smoothing_episodes"]
        smooth = [returns[max(0, i-window+1):i+1].mean() for i in range(len(returns))]
        axes[1].plot(ex, smooth, color="#7857a7", label=f"Up to {window}-episode moving mean")
        axes[1].plot(x, mean/100, "o-", color="#2368a0", label="Frozen evaluation mean")
        axes[2].plot(ex, [e["training_champion_reduction"]*100 for e in episodes], color="#df8640", label="Training discovery record")
        axes[2].plot(x, [s["improvement_frequency"]*100 for s in summaries], "o-", color="#009b77", label="Evaluation improvement frequency")
    axes[1].set(title="Episode reward", ylabel="Undiscounted task return", xlabel="Training commands")
    if episodes:
        axes[1].legend(fontsize=8)
    axes[2].set(title="Discovery and repeatability", ylabel="Percent (separate measures)", xlabel="Training commands")
    if episodes:
        axes[2].legend(fontsize=8)
    for ax in axes:
        ax.grid(alpha=.2)
        ax.ticklabel_format(axis="x", style="sci", scilimits=(0, 0))
    count = episodes[-1]["commands"] if episodes else 0
    fig.suptitle(f"{metadata['device'].upper()} learner / CPU actor | {metadata['parameters']:,} parameters | {count:,} commands | one training seed\n"
                 "Same initial layout and 128 edits per episode; raw episode reward equals relative area reduction", fontsize=12)
    fig.savefig(output/"learning-curves.png", dpi=170)
    fig.savefig(output/"learning-curves.svg")
    plt.close(fig)

    choices = [("initial.json", "Initial factory"),
               ("evaluation-median.json", "Final checkpoint: median evaluation outcome"),
               ("training-best.json", "Best discovered during training"),
               ("witness.json", "Scripted environment witness (not learned)")]
    layouts = [(json.loads((output/name).read_text()), title) for name, title in choices if (output/name).exists()]
    layout_bounds = [check(catalog, layout)["bounds"] for layout, _ in layouts]
    common = (min(b[0] for b in layout_bounds), min(b[1] for b in layout_bounds),
              max(b[2] for b in layout_bounds), max(b[3] for b in layout_bounds))
    fig, axes = plt.subplots(1, len(layouts), figsize=(4*len(layouts), 8), constrained_layout=True, squeeze=False)
    network_colors = {nid: plt.get_cmap("tab10")(i) for i, nid in enumerate(catalog.networks)}
    for ax, (layout, title) in zip(axes[0], layouts):
        validation = check(catalog, layout)
        if not validation["valid"]:
            raise ValueError(f"Refusing a complete-factory visualization: {title}: {validation}")
        for fid, pose in layout["placements"].items():
            body = footprint(catalog, fid, pose)
            xs, ys = zip(*body)
            x, y, w, h = min(xs), min(ys), max(xs)-min(xs)+1, max(ys)-min(ys)+1
            ax.add_patch(Rectangle((x-.5, y-.5), w, h, facecolor="#dce7ef", edgecolor="#354b60", linewidth=1.5))
            ax.text(x+(w-1)/2, y+(h-1)/2, fid, ha="center", va="center", weight="bold", fontsize=15)
        for nid, network in layout["networks"].items():
            color = network_colors[nid]
            for x, y in network["cells"]:
                ax.add_patch(Rectangle((x-.38, y-.38), .76, .76, facecolor=color, alpha=.75))
            for a, b in network["edges"]:
                ax.annotate("", xy=b, xytext=a, arrowprops={"arrowstyle": "->", "color": "#263844", "lw": 1})
            for role in ("source", "target"):
                binding = layout["bindings"][f"{nid}:{role}"]
                if "cell" in binding:
                    ax.scatter(*binding["cell"], marker="o" if role == "source" else "s", s=62,
                               facecolors="white", edgecolors="black", zorder=5)
        x0, y0, x1, y1 = validation["bounds"]
        ax.add_patch(Rectangle((x0-.5, y0-.5), x1-x0+1, y1-y0+1, fill=False, linestyle="--", edgecolor="#ba5260"))
        area, tiles, turns = validation["score"]
        ax.set_title(f"{title}\n{x1-x0+1} x {y1-y0+1} | area {area}, belt tiles {tiles}, turns {turns}", fontsize=10)
        ax.set_aspect("equal")
        ax.set_xlim(common[0]-1.5, common[2]+1.5)
        ax.set_ylim(common[3]+1.5, common[1]-1.5)
        ax.set_xticks(range(common[0], common[2]+1, 2))
        ax.set_yticks(range(common[1], common[3]+1, 2))
        ax.grid(alpha=.12)
    labels = ", ".join(f"{fid}: {f['recipe']}" for fid, f in catalog.facilities.items())
    fig.suptitle("Complete validated factories | all original production obligations preserved\n"+labels, fontsize=10)
    fig.savefig(output/"factory-comparison.png", dpi=160)
    fig.savefig(output/"factory-comparison.svg")
    plt.close(fig)
    return {"curves": str(output/"learning-curves.png"), "layouts": str(output/"factory-comparison.png")}


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    print(json.dumps(render(args.output, Catalog(args.root, args.case)), indent=2))
