"""Run and summarize a paired search-policy target audit."""

import argparse
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import time

from ppo_harness_state import read, sha, write


STREAMS = ("s1", "s2", "s3")
BUDGETS = (128, 512)
LOWER_SHA = "353d560842da6c4270605da552a04adc8d96411512d8d3b847625cf9c502dea9"
OUTER_SHA = "6a1f61578aa29a9e80d90ab7805bfec2ef8e543bd24fd9db90441fbb5d724520"


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-target-quality: {message}")


def config(budget):
    if budget == 128:
        return Path("docs/benchmarks/hierarchical-policy-stability/search.json")
    return Path("docs/benchmarks/hierarchical-target-quality/search-512.json")


def model(root, budget):
    return root / "inputs" / "models" / f"outer-{budget}"


def panel(root, stream):
    return root / "inputs" / "panels" / f"{stream}.json"


def prepare(root, experiment_commit):
    require(root.is_dir() and not (root / "protocol.json").exists(),
            "audit inputs are missing or protocol already exists")
    search128, search512 = read(config(128)), read(config(512))
    expected512 = dict(search128, simulations_per_move=512)
    require(search128["simulations_per_move"] == 128 and search512 == expected512,
            "search configs differ beyond simulation budget")
    outer128 = read(model(root, 128) / "config.json")
    outer512 = read(model(root, 512) / "config.json")
    require(outer128["search"] == search128 and outer512["search"] == search512
            and dict(outer128, search=search512) == outer512,
            "outer configs differ beyond search budget")
    require(sha(root / "inputs" / "models" / "lower" / "state.safetensors") == LOWER_SHA
            and all(sha(model(root, budget) / "state.safetensors") == OUTER_SHA
                    for budget in BUDGETS), "starting model weights differ")
    identities = set()
    import hashlib
    for stream in STREAMS:
        loaded = read(panel(root, stream))
        require(loaded["partition"] == "training" and len(loaded["cases"]) == 16,
                f"wrong panel: {stream}")
        for case in loaded["cases"]:
            identity = hashlib.sha256(json.dumps(case["problem"], sort_keys=True,
                separators=(",", ":")).encode()).hexdigest()
            require(identity not in identities, "duplicate problem identity across audit panels")
            identities.add(identity)
    protocol = {
        "schema": "aic-hierarchical-target-quality-protocol-v1",
        "experiment_commit": experiment_commit,
        "training_source_commit": "31555f5",
        "release_binary_sha256": sha(Path("target/release/aic-cli")),
        "sampling_sha256": sha(Path("docs/benchmarks/direct-constructive-full-graphs/sampling.json")),
        "search_config_sha256": {str(budget): sha(config(budget)) for budget in BUDGETS},
        "outer_config_sha256": {str(budget): sha(model(root, budget) / "config.json")
                                 for budget in BUDGETS},
        "lower_checkpoint_sha256": LOWER_SHA,
        "outer_checkpoint_sha256": OUTER_SHA,
        "panel_sha256": {stream: sha(panel(root, stream)) for stream in STREAMS},
        "problem_identities": len(identities),
        "planned_cells": 6,
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {
        "schema": "aic-hierarchical-target-quality-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared",
        "records": [],
    })
    print(f"prepared_protocol_sha256={sha(root / 'protocol.json')}", flush=True)


def run(root):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    require(progress["protocol_sha256"] == sha(root / "protocol.json"),
            "protocol changed")
    require(sha(Path("target/release/aic-cli")) == protocol["release_binary_sha256"],
            "binary changed")
    require(all(sha(panel(root, stream)) == protocol["panel_sha256"][stream]
                for stream in STREAMS), "panel changed")
    require(all(sha(config(budget)) == protocol["search_config_sha256"][str(budget)]
                and sha(model(root, budget) / "config.json")
                == protocol["outer_config_sha256"][str(budget)]
                and sha(model(root, budget) / "state.safetensors") == OUTER_SHA
                for budget in BUDGETS), "search contract or weights changed")
    progress["status"] = "running"
    write(root / "progress.json", progress)
    logs = root / "logs"
    logs.mkdir(exist_ok=True)
    for stream in STREAMS:
        for budget in BUDGETS:
            if any(row["stream"] == stream and row["simulations"] == budget
                   for row in progress["records"]):
                continue
            output = root / "raw" / stream / f"{budget}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            seconds = None
            if not output.exists():
                command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                           "--config", str(config(budget)), "--panel", str(panel(root, stream)),
                           "--lower-training-output", str(root / "inputs" / "models" / "lower"),
                           "--outer-training-output", str(model(root, budget)),
                           "--catalog-root", "data/game/normalized", "--output", str(output),
                           "--device", "cuda:0"]
                started = time.monotonic()
                with (logs / f"{stream}-{budget}.log").open("w") as log:
                    result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                            check=False)
                require(result.returncode == 0, f"search failed: {stream}-{budget}")
                seconds = time.monotonic() - started
            report = read(output)
            require(report["status"] == "complete" and report["partition"] == "training"
                    and report["root_records_retained"] and len(report["cases"]) == 16
                    and report["panel_sha256"] == protocol["panel_sha256"][stream]
                    and report["lower_checkpoint_sha256"] == LOWER_SHA
                    and report["outer_checkpoint_sha256"] == OUTER_SHA,
                    f"invalid raw report: {stream}-{budget}")
            del report
            row = {"stream": stream, "simulations": budget, "report_sha256": sha(output),
                   "report_bytes": output.stat().st_size, "integrated_wall_seconds": seconds}
            progress["records"].append(row)
            write(root / "progress.json", progress)
            print(f"completed={stream}-{budget} bytes={row['report_bytes']}", flush=True)
    require(len(progress["records"]) == 6, "missing audit cell")
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def action_key(action):
    return json.dumps(action["action"], sort_keys=True, separators=(",", ":"))


def mean(values):
    return sum(values) / len(values) if values else None


def entropy(values):
    return -sum(value * math.log(value) for value in values if value > 0)


def js_divergence(left, right):
    middle = [(a + b) / 2 for a, b in zip(left, right)]
    return (sum(a * math.log2(a / m) for a, m in zip(left, middle) if a > 0)
            + sum(b * math.log2(b / m) for b, m in zip(right, middle) if b > 0)) / 2


def paired_case(stream, left, right):
    require(left["id"] == right["id"], f"case order differs: {stream}")
    low, high = left["mcts"], right["mcts"]
    result = {"stream": stream, "case_id": left["id"],
              "completed_128": bool(low["fully_validated"]),
              "completed_512": bool(high["fully_validated"])}
    if not low["root_records"] or not high["root_records"]:
        require(not low["root_records"] and not high["root_records"],
                f"first-root coverage differs: {stream}/{left['id']}")
        result["covered"] = False
        return result
    a, b = low["root_records"][0], high["root_records"][0]
    require(a["state_sha256"] == b["state_sha256"]
            and a["state_revision"] == b["state_revision"],
            f"first-root state differs: {stream}/{left['id']}")
    left_actions = {action_key(action): action for action in a["actions"]}
    right_actions = {action_key(action): action for action in b["actions"]}
    require(len(left_actions) == len(a["actions"])
            and len(right_actions) == len(b["actions"])
            and left_actions.keys() == right_actions.keys(),
            f"first-root action set differs: {stream}/{left['id']}")
    keys = list(left_actions)
    p = [left_actions[key]["policy_target"] for key in keys]
    q = [right_actions[key]["policy_target"] for key in keys]
    mixed_prior = [(left_actions[key]["prior"] + left_actions[key]["anchor_prior"]) / 2
                   for key in keys]
    prior_total = sum(mixed_prior)
    require(prior_total > 0, f"empty mixed prior: {stream}/{left['id']}")
    mixed_prior = [value / prior_total for value in mixed_prior]
    require(all(value is not None and value >= 0 for value in p + q)
            and abs(sum(p) - 1) < 1e-6 and abs(sum(q) - 1) < 1e-6,
            f"invalid target distribution: {stream}/{left['id']}")
    top_p = max(range(len(keys)), key=lambda index: p[index])
    top_q = max(range(len(keys)), key=lambda index: q[index])
    entropy_p = entropy(p)
    entropy_q = entropy(q)
    selected_high = action_key(b["actions"][b["selected_action"]])
    selected_low = action_key(a["actions"][a["selected_action"]])
    high_index = keys.index(selected_high)
    result.update({
        "covered": True,
        "state_sha256": a["state_sha256"],
        "actions": len(keys),
        "visited_128": sum(left_actions[key]["visits"] > 0 for key in keys),
        "visited_512": sum(right_actions[key]["visits"] > 0 for key in keys),
        "unvisited_target_mass_128": sum(p[index] for index, key in enumerate(keys)
                                         if left_actions[key]["visits"] == 0),
        "uniform_unvisited_mass_128": (len(keys) - sum(left_actions[key]["visits"] > 0
                                                        for key in keys)) / len(keys),
        "top_target_unvisited_128": left_actions[keys[top_p]]["visits"] == 0,
        "top_target_probability_128": p[top_p],
        "js_target_vs_mixed_prior_128_bits": js_divergence(p, mixed_prior),
        "target_entropy_128": entropy_p,
        "target_entropy_512": entropy_q,
        "effective_actions_128": math.exp(entropy_p),
        "effective_actions_512": math.exp(entropy_q),
        "top_target_same": keys[top_p] == keys[top_q],
        "selected_action_same": selected_low == selected_high,
        "mass_128_on_selected_512": p[high_index],
        "rank_128_of_selected_512": 1 + sum(value > p[high_index] for value in p),
        "js_divergence_bits": js_divergence(p, q),
    })
    return result


def summarize(root):
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    require(progress["status"] == "complete" and len(progress["records"]) == 6
            and progress["protocol_sha256"] == sha(root / "protocol.json"),
            "audit is incomplete")
    rows = []
    for stream in STREAMS:
        reports = {}
        for budget in BUDGETS:
            path = root / "raw" / stream / f"{budget}.json"
            record = next(row for row in progress["records"]
                          if row["stream"] == stream and row["simulations"] == budget)
            require(sha(path) == record["report_sha256"],
                    f"report changed: {stream}-{budget}")
            reports[budget] = read(path)
        for left, right in zip(reports[128]["cases"], reports[512]["cases"]):
            rows.append(paired_case(stream, left, right))
        del reports
    require(len(rows) == 48, "wrong paired case count")
    panel_summaries = {}
    for stream in STREAMS:
        group = [row for row in rows if row["stream"] == stream]
        covered = [row for row in group if row["covered"]]
        require(covered, f"no paired first roots: {stream}")
        panel_summaries[stream] = {
            "cases": len(group),
            "covered": len(covered),
            "completed_128": sum(row["completed_128"] for row in group),
            "completed_512": sum(row["completed_512"] for row in group),
            "gained_512": sum(row["completed_512"] and not row["completed_128"]
                              for row in group),
            "lost_512": sum(row["completed_128"] and not row["completed_512"]
                            for row in group),
            "mean_unvisited_target_mass_128": mean([row["unvisited_target_mass_128"]
                                                     for row in covered]),
            "mean_visited_actions_128": mean([row["visited_128"] for row in covered]),
            "mean_visited_actions_512": mean([row["visited_512"] for row in covered]),
            "mean_action_count": mean([row["actions"] for row in covered]),
            "mean_effective_actions_128": mean([row["effective_actions_128"]
                                                for row in covered]),
            "mean_effective_actions_512": mean([row["effective_actions_512"]
                                                for row in covered]),
            "mean_uniform_unvisited_mass_128": mean([row["uniform_unvisited_mass_128"]
                                                     for row in covered]),
            "mean_top_target_probability_128": mean([row["top_target_probability_128"]
                                                       for row in covered]),
            "mean_js_target_vs_mixed_prior_128_bits": mean([
                row["js_target_vs_mixed_prior_128_bits"] for row in covered]),
            "top_target_unvisited_128": sum(row["top_target_unvisited_128"]
                                            for row in covered),
            "top1_disagreement": sum(not row["top_target_same"] for row in covered),
            "mean_js_divergence_bits": mean([row["js_divergence_bits"] for row in covered]),
            "successful_128": {
                "covered": sum(row["completed_128"] for row in covered),
                "mean_unvisited_target_mass": mean([row["unvisited_target_mass_128"]
                    for row in covered if row["completed_128"]]),
                "top1_disagreement": sum(not row["top_target_same"]
                                          for row in covered if row["completed_128"]),
            },
            "failed_128": {
                "covered": sum(not row["completed_128"] for row in covered),
                "mean_unvisited_target_mass": mean([row["unvisited_target_mass_128"]
                    for row in covered if not row["completed_128"]]),
                "top1_disagreement": sum(not row["top_target_same"]
                                          for row in covered if not row["completed_128"]),
            },
        }
    covered = [row for row in rows if row["covered"]]
    weak = sum(panel_summaries[stream]["mean_unvisited_target_mass_128"] > 0.5
               for stream in STREAMS) >= 2
    disagreement = sum(not row["top_target_same"] for row in covered) / len(covered)
    summary = {
        "schema": "aic-hierarchical-target-quality-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "panels": panel_summaries,
        "overall": {
            "covered": len(covered),
            "mean_unvisited_target_mass_128": mean([row["unvisited_target_mass_128"]
                                                     for row in covered]),
            "top1_disagreement_fraction": disagreement,
            "mean_mass_128_on_selected_512": mean([row["mass_128_on_selected_512"]
                                                    for row in covered]),
            "median_rank_128_of_selected_512": statistics.median(
                row["rank_128_of_selected_512"] for row in covered),
            "mean_js_divergence_bits": mean([row["js_divergence_bits"] for row in covered]),
            "mean_js_target_vs_mixed_prior_128_bits": mean([
                row["js_target_vs_mixed_prior_128_bits"] for row in covered]),
        },
        "predeclared_signals": {
            "weakly_grounded": weak,
            "budget_sensitive": disagreement > 0.25,
        },
        "cases": rows,
    }
    write(root / "summary.json", summary)
    print(json.dumps({"panels": panel_summaries, "overall": summary["overall"],
                      "signals": summary["predeclared_signals"]}, indent=2), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--experiment-commit")
    args = parser.parse_args()
    os.chdir(Path(__file__).resolve().parents[2])
    if args.command == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    elif args.command == "run":
        run(args.output)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
