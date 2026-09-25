"""Paired candidate-width search on the frozen target-audit panels."""

import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics
import subprocess
import time

from ppo_harness_state import read, sha, write


STREAMS = ("s1", "s2", "s3")
ARMS = ((16, 128), (32, 128), (64, 128), (16, 512), (32, 512),
        (64, 512), (128, 512))
CONFIG_ROOT = Path("docs/benchmarks/hierarchical-target-causal")
PREVIOUS = read(Path("docs/reports/2026-09-25-hierarchical-target-quality-protocol.json"))


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-candidate-width: {message}")


def label(candidates, simulations):
    return f"c{candidates}-n{simulations}"


def panel(root, stream):
    return root / "inputs" / "panels" / f"{stream}.json"


def model(root, arm):
    return root / "inputs" / "models" / f"outer-{arm}"


def action_key(action):
    return json.dumps(action["action"], sort_keys=True, separators=(",", ":"))


def write_configs():
    base_search = read(Path("docs/benchmarks/hierarchical-policy-stability/search.json"))
    base_outer = read(Path("docs/benchmarks/hierarchical-policy-stability/outer-initial.json"))
    require(base_outer["search"] == base_search, "canonical outer/search mismatch")
    require(base_search["simulations_per_move"] == 128
            and base_search["root_selection"]["candidates"] == 16,
            "canonical search changed")
    CONFIG_ROOT.mkdir(parents=True, exist_ok=True)
    for candidates, simulations in ARMS:
        arm = label(candidates, simulations)
        search = json.loads(json.dumps(base_search))
        search["simulations_per_move"] = simulations
        search["root_selection"]["candidates"] = candidates
        outer = dict(base_outer, search=search)
        write(CONFIG_ROOT / f"{arm}-search.json", search)
        write(CONFIG_ROOT / f"{arm}-outer.json", outer)
    print(f"written_configurations={len(ARMS)}")


def prepare(root, commit):
    require(root.is_dir() and not (root / "protocol.json").exists(),
            "prepare inputs first, once, in a fresh output root")
    input_models = root / "inputs" / "models"
    require(sha(input_models / "lower" / "state.safetensors")
            == PREVIOUS["lower_checkpoint_sha256"], "lower checkpoint changed")
    require(sha(input_models / "outer-128" / "state.safetensors")
            == PREVIOUS["outer_checkpoint_sha256"], "outer checkpoint changed")
    for stream in STREAMS:
        require(sha(panel(root, stream)) == PREVIOUS["panel_sha256"][stream],
                f"panel changed: {stream}")
    config_hashes = {}
    for candidates, simulations in ARMS:
        arm = label(candidates, simulations)
        search_path = CONFIG_ROOT / f"{arm}-search.json"
        outer_path = CONFIG_ROOT / f"{arm}-outer.json"
        search, outer = read(search_path), read(outer_path)
        require(outer["search"] == search
                and search["root_selection"]["candidates"] == candidates
                and search["root_selection"]["anchor_candidates"] == 8
                and search["simulations_per_move"] == simulations,
                f"invalid arm configuration: {arm}")
        directory = model(root, arm)
        if not directory.exists():
            directory.mkdir()
            for name in ("state.safetensors", "parameter-order.json"):
                (directory / name).symlink_to(Path("..") / "outer-128" / name)
        write(directory / "config.json", outer)
        config_hashes[arm] = {"search": sha(search_path), "outer": sha(outer_path),
                              "model_config": sha(directory / "config.json")}
    protocol = {
        "schema": "aic-hierarchical-candidate-width-protocol-v1",
        "experiment_commit": commit,
        "binary_sha256": sha(Path("target/release/aic-cli")),
        "panels": {stream: sha(panel(root, stream)) for stream in STREAMS},
        "lower_checkpoint_sha256": PREVIOUS["lower_checkpoint_sha256"],
        "outer_checkpoint_sha256": PREVIOUS["outer_checkpoint_sha256"],
        "configs": config_hashes,
        "planned_cells": len(STREAMS) * len(ARMS),
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {"schema": "aic-hierarchical-candidate-width-progress-v1",
                                   "protocol_sha256": sha(root / "protocol.json"),
                                   "records": [], "status": "prepared"})
    print(f"prepared_protocol_sha256={sha(root / 'protocol.json')}")


def check_inputs(root, protocol):
    require(sha(Path("target/release/aic-cli")) == protocol["binary_sha256"],
            "executable changed")
    for stream in STREAMS:
        require(sha(panel(root, stream)) == protocol["panels"][stream],
                f"panel changed: {stream}")
    for candidates, simulations in ARMS:
        arm = label(candidates, simulations)
        hashes = protocol["configs"][arm]
        require(sha(CONFIG_ROOT / f"{arm}-search.json") == hashes["search"]
                and sha(CONFIG_ROOT / f"{arm}-outer.json") == hashes["outer"]
                and sha(model(root, arm) / "config.json") == hashes["model_config"]
                and sha(model(root, arm) / "state.safetensors")
                == protocol["outer_checkpoint_sha256"],
                f"config or model changed: {arm}")


def run(root):
    protocol, progress = read(root / "protocol.json"), read(root / "progress.json")
    require(progress["protocol_sha256"] == sha(root / "protocol.json"),
            "protocol changed")
    check_inputs(root, protocol)
    progress["status"] = "running"
    write(root / "progress.json", progress)
    (root / "logs").mkdir(exist_ok=True)
    for stream in STREAMS:
        for candidates, simulations in ARMS:
            arm = label(candidates, simulations)
            if any(row["stream"] == stream and row["arm"] == arm
                   for row in progress["records"]):
                continue
            output = root / "raw" / stream / f"{arm}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            seconds = None
            if not output.exists():
                command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                           "--config", str(CONFIG_ROOT / f"{arm}-search.json"),
                           "--panel", str(panel(root, stream)),
                           "--lower-training-output", str(root / "inputs/models/lower"),
                           "--outer-training-output", str(model(root, arm)),
                           "--catalog-root", "data/game/normalized", "--output", str(output),
                           "--device", "cuda:0"]
                started = time.monotonic()
                with (root / "logs" / f"{stream}-{arm}.log").open("w") as log:
                    completed = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                               check=False)
                require(completed.returncode == 0, f"search failed: {stream}/{arm}")
                seconds = time.monotonic() - started
            report = read(output)
            require(report["status"] == "complete" and report["partition"] == "training"
                    and report["root_records_retained"]
                    and report["panel_sha256"] == protocol["panels"][stream]
                    and report["lower_checkpoint_sha256"]
                    == protocol["lower_checkpoint_sha256"]
                    and report["outer_checkpoint_sha256"]
                    == protocol["outer_checkpoint_sha256"]
                    and len(report["cases"]) == 16, f"bad report: {stream}/{arm}")
            progress["records"].append({"stream": stream, "arm": arm,
                                        "report_sha256": sha(output),
                                        "report_bytes": output.stat().st_size,
                                        "wall_seconds": seconds})
            write(root / "progress.json", progress)
            print(f"completed={stream}/{arm} seconds={seconds}", flush=True)
    progress["status"] = "complete"
    write(root / "progress.json", progress)


def entropy(weights):
    return -sum(weight * math.log(weight) for weight in weights if weight > 0)


def js_divergence(left, right):
    middle = [(a + b) / 2 for a, b in zip(left, right)]
    return (sum(a * math.log2(a / m) for a, m in zip(left, middle) if a > 0)
            + sum(b * math.log2(b / m) for b, m in zip(right, middle) if b > 0)) / 2


def summarize_case(stream, reports, index):
    base = reports["c16-n128"]["cases"][index]
    baseline = base["mcts"]["root_records"][0]
    actions = {action_key(action): action for action in baseline["actions"]}
    require(len(actions) == len(baseline["actions"]),
            f"duplicate baseline action: {stream}/{base['id']}")
    base_visits = {key for key, action in actions.items() if action["visits"] > 0}
    visited_by_arm = {}
    row = {"stream": stream, "case_id": base["id"],
           "state_sha256": baseline["state_sha256"], "actions": len(actions), "arms": {}}
    for candidates, simulations in ARMS:
        arm = label(candidates, simulations)
        case = reports[arm]["cases"][index]
        require(case["id"] == base["id"], f"case order differs: {stream}/{arm}")
        root = case["mcts"]["root_records"][0]
        current = {action_key(action): action for action in root["actions"]}
        require(root["state_sha256"] == baseline["state_sha256"]
                and root["state_revision"] == baseline["state_revision"]
                and len(current) == len(actions) and current.keys() == actions.keys(),
                f"first-root action set differs: {stream}/{arm}/{case['id']}")
        visited = {key for key, action in current.items() if action["visits"] > 0}
        require(len(visited) == min(candidates, len(actions)),
                f"visited width differs: {stream}/{arm}/{case['id']}")
        visited_by_arm[arm] = visited
        selected = action_key(root["actions"][root["selected_action"]])
        targets = [action["policy_target"] for action in root["actions"]]
        priors = [0.5 * (action["prior"] + action["anchor_prior"])
                  for action in root["actions"]]
        total = sum(priors)
        priors = [prior / total for prior in priors]
        require(all(target is not None and target >= 0 for target in targets)
                and abs(sum(targets) - 1) < 1e-6,
                f"invalid target: {stream}/{arm}/{case['id']}")
        result = case["mcts"]
        row["arms"][arm] = {
            "selected_action_sha256": hashlib.sha256(selected.encode()).hexdigest(),
            "selected_in_baseline_16": selected in base_visits,
            "selected_q": current[selected]["q"],
            "selected_visits": current[selected]["visits"],
            "visited_actions": len(visited),
            "visited_overlap_baseline_16": len(visited & base_visits),
            "completed": result["fully_validated"],
            "completed_connections": result["completed_connections"],
            "total_connections": result["total_connections"],
            "footprint_area": result["footprint_area"],
            "transport_cells": result["transport_cells"],
            "turns": result["turns"],
            "elapsed_seconds": result["elapsed_seconds"],
            "target_effective_actions": math.exp(entropy(targets)),
            "unvisited_target_mass": sum(action["policy_target"]
                                         for action in root["actions"]
                                         if action["visits"] == 0),
            "target_prior_js_bits": js_divergence(targets, priors),
            "top_target_probability": max(targets),
        }
    for simulations in (128, 512):
        widths = (16, 32, 64) if simulations == 128 else (16, 32, 64, 128)
        for narrow, wide in zip(widths, widths[1:]):
            require(visited_by_arm[label(narrow, simulations)]
                    <= visited_by_arm[label(wide, simulations)],
                    f"candidate sets are not nested: {stream}/{case['id']}/{simulations}")
    chosen = []
    for arm in ("c16-n128", "c64-n128", "c64-n512", "c128-n512"):
        root = reports[arm]["cases"][index]["mcts"]["root_records"][0]
        chosen.append(action_key(root["actions"][root["selected_action"]]))
    outside = [(key, action) for key, action in actions.items() if key not in base_visits]
    if outside:
        chosen.append(max(outside, key=lambda item: item[1]["anchor_prior"])[0])
        chosen.append(max(outside, key=lambda item:
                          item[1]["prior"] + item[1]["anchor_prior"])[0])
    row["counterfactual_candidates"] = []
    for key in dict.fromkeys(chosen):
        evidence = {"action_sha256": hashlib.sha256(key.encode()).hexdigest(),
                    "action": actions[key]["action"],
                    "in_baseline_16": key in base_visits, "arms": {}}
        for candidates, simulations in ARMS:
            arm = label(candidates, simulations)
            root = reports[arm]["cases"][index]["mcts"]["root_records"][0]
            action = next(action for action in root["actions"] if action_key(action) == key)
            evidence["arms"][arm] = {"visits": action["visits"], "q": action["q"],
                                     "policy_target": action["policy_target"]}
        row["counterfactual_candidates"].append(evidence)
    return row


def summarize(root):
    protocol, progress = read(root / "protocol.json"), read(root / "progress.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"]
            and progress["protocol_sha256"] == sha(root / "protocol.json"),
            "search matrix incomplete")
    cases = []
    for stream in STREAMS:
        reports = {}
        for candidates, simulations in ARMS:
            arm = label(candidates, simulations)
            path = root / "raw" / stream / f"{arm}.json"
            record = next(row for row in progress["records"]
                          if row["stream"] == stream and row["arm"] == arm)
            require(sha(path) == record["report_sha256"],
                    f"raw report changed: {stream}/{arm}")
            reports[arm] = read(path)
        for index in range(16):
            cases.append(summarize_case(stream, reports, index))
    arms = {}
    for candidates, simulations in ARMS:
        arm = label(candidates, simulations)
        values = [case["arms"][arm] for case in cases]
        baseline = [case["arms"]["c16-n128"] for case in cases]
        arms[arm] = {
            "completed": sum(value["completed"] for value in values),
            "gained_vs_c16_n128": sum(value["completed"] and not base["completed"]
                                     for value, base in zip(values, baseline)),
            "lost_vs_c16_n128": sum(base["completed"] and not value["completed"]
                                   for value, base in zip(values, baseline)),
            "selected_outside_baseline_16": sum(not value["selected_in_baseline_16"]
                                                for value in values),
            "mean_target_effective_actions": statistics.mean(
                value["target_effective_actions"] for value in values),
            "mean_unvisited_target_mass": statistics.mean(
                value["unvisited_target_mass"] for value in values),
            "mean_target_prior_js_bits": statistics.mean(
                value["target_prior_js_bits"] for value in values),
            "mean_case_elapsed_seconds": statistics.mean(
                value["elapsed_seconds"] for value in values),
        }
    output = {"schema": "aic-hierarchical-candidate-width-summary-v1",
              "protocol_sha256": sha(root / "protocol.json"),
              "progress_sha256": sha(root / "progress.json"),
              "cases": cases, "arms": arms}
    write(root / "summary.json", output)
    print(json.dumps(arms, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("write-configs", "prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    parser.add_argument("--experiment-commit")
    args = parser.parse_args()
    if args.operation == "write-configs":
        write_configs()
    elif args.operation == "prepare":
        require(args.experiment_commit is not None, "--experiment-commit required")
        prepare(args.output, args.experiment_commit)
    elif args.operation == "run":
        run(args.output)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
