"""Measure first-root policy-target agreement across independent search seeds."""

import argparse
import hashlib
import itertools
import json
import math
from pathlib import Path
import statistics
import subprocess
import time

from hierarchical_candidate_width import CONFIG_ROOT, STREAMS, action_key
from hierarchical_target_contrast import VARIANTS, targets
from ppo_harness_state import read, sha, write


SEEDS = (20260922, 20260923, 20260924)


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-target-seed: {message}")


def config(seed, kind):
    return CONFIG_ROOT / f"target-seed-{seed}-{kind}.json"


def write_configs():
    search = read(CONFIG_ROOT / "c16-n128-search.json")
    outer = read(CONFIG_ROOT / "c16-n128-outer.json")
    require(outer["search"] == search, "source configs differ")
    for seed in SEEDS:
        trial = json.loads(json.dumps(search))
        trial["max_moves"] = 1
        trial["root_selection"]["seed"] = seed
        write(config(seed, "search"), trial)
        write(config(seed, "outer"), dict(outer, search=trial))
    print(f"written_target_seed_configs={len(SEEDS)}")


def prepare(root, experiment_commit):
    require(not (root / "target-seed-protocol.json").exists(),
            "target seed audit already prepared")
    replication = read(root / "replication-summary.json")
    replicate_progress = read(root / "replication-progress.json")
    require(replication["progress_sha256"] == sha(root / "replication-progress.json")
            and replicate_progress["status"] == "complete",
            "independent continuation replication is not final")
    panels = [read(root / "inputs/panels" / f"{stream}.json") for stream in STREAMS]
    cases = [case for panel in panels for case in panel["cases"]]
    require(len(cases) == 48 and len({case["id"] for case in cases}) == 48,
            "source cases differ")
    panel = dict(panels[0], cases=cases)
    panel_path = root / "inputs/target-seed-panel.json"
    write(panel_path, panel)
    hashes = {}
    for seed in SEEDS:
        search = read(config(seed, "search"))
        outer = read(config(seed, "outer"))
        require(search["max_moves"] == 1
                and search["simulations_per_move"] == 128
                and search["root_selection"]["candidates"] == 16
                and search["root_selection"]["seed"] == seed
                and outer["search"] == search,
                f"target seed config differs: {seed}")
        model = root / "inputs/models" / f"outer-target-seed-{seed}"
        model.mkdir(parents=True, exist_ok=True)
        for name in ("state.safetensors", "parameter-order.json"):
            link = model / name
            if not link.exists():
                link.symlink_to(Path("..") / "outer-128" / name)
        write(model / "config.json", outer)
        hashes[str(seed)] = {"search": sha(config(seed, "search")),
                             "outer": sha(config(seed, "outer")),
                             "model_config": sha(model / "config.json")}
    protocol = {
        "schema": "aic-hierarchical-target-seed-protocol-v1",
        "experiment_commit": experiment_commit,
        "runner_sha256": sha(Path(__file__)),
        "target_transform_sha256": sha(Path(__file__).with_name(
            "hierarchical_target_contrast.py")),
        "replication_summary_sha256": sha(root / "replication-summary.json"),
        "actions_sha256": sha(root / "continuation-actions.json"),
        "baseline_report_sha256": {stream: sha(root / "raw" / stream / "c16-n128.json")
                                   for stream in STREAMS},
        "source_panel_sha256": {stream: sha(root / "inputs/panels" / f"{stream}.json")
                                for stream in STREAMS},
        "panel_sha256": sha(panel_path), "configs": hashes,
        "binary_sha256": sha(Path("target/release/aic-cli")),
        "lower_checkpoint_sha256": sha(root / "inputs/models/lower/state.safetensors"),
        "outer_checkpoint_sha256": sha(root / "inputs/models/outer-128/state.safetensors"),
        "planned_cells": len(SEEDS),
    }
    write(root / "target-seed-protocol.json", protocol)
    write(root / "target-seed-progress.json", {
        "schema": "aic-hierarchical-target-seed-progress-v1",
        "protocol_sha256": sha(root / "target-seed-protocol.json"),
        "records": [], "status": "prepared"})
    print("prepared_target_seed_cells=3 cases=48")


def run(root, max_cells):
    protocol = read(root / "target-seed-protocol.json")
    progress = read(root / "target-seed-progress.json")
    require(progress["protocol_sha256"] == sha(root / "target-seed-protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["target_transform_sha256"] == sha(Path(__file__).with_name(
                "hierarchical_target_contrast.py"))
            and protocol["binary_sha256"] == sha(Path("target/release/aic-cli"))
            and protocol["panel_sha256"] == sha(root / "inputs/target-seed-panel.json")
            and protocol["actions_sha256"] == sha(root / "continuation-actions.json"),
            "target seed inputs changed")
    progress["status"] = "running"
    write(root / "target-seed-progress.json", progress)
    completed = 0
    for seed in SEEDS:
        if any(row["seed"] == seed for row in progress["records"]):
            continue
        hashes = protocol["configs"][str(seed)]
        model = root / "inputs/models" / f"outer-target-seed-{seed}"
        require(sha(config(seed, "search")) == hashes["search"]
                and sha(config(seed, "outer")) == hashes["outer"]
                and sha(model / "config.json") == hashes["model_config"]
                and sha(model / "state.safetensors")
                == protocol["outer_checkpoint_sha256"],
                f"target seed model changed: {seed}")
        output = root / "raw/target-seed" / str(seed) / "first-root.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        log = root / "logs" / f"target-seed-{seed}.log"
        started = time.monotonic()
        if not output.exists():
            command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                       "--config", str(config(seed, "search")),
                       "--panel", str(root / "inputs/target-seed-panel.json"),
                       "--lower-training-output", str(root / "inputs/models/lower"),
                       "--outer-training-output", str(model),
                       "--catalog-root", "data/game/normalized",
                       "--output", str(output), "--device", "cuda:0"]
            with log.open("w") as stream:
                result = subprocess.run(command, stdout=stream,
                                        stderr=subprocess.STDOUT, check=False)
            require(result.returncode == 0, f"target seed search failed: {seed}")
        report = read(output)
        require(report["status"] == "complete" and report["partition"] == "training"
                and report["panel_sha256"] == protocol["panel_sha256"]
                and report["lower_checkpoint_sha256"]
                == protocol["lower_checkpoint_sha256"]
                and report["outer_checkpoint_sha256"]
                == protocol["outer_checkpoint_sha256"]
                and len(report["cases"]) == 48,
                f"target seed report differs: {seed}")
        progress["records"].append({"seed": seed,
            "report_sha256": sha(output), "report_bytes": output.stat().st_size,
            "wall_seconds": time.monotonic() - started})
        write(root / "target-seed-progress.json", progress)
        print(f"completed_target_seed={seed}", flush=True)
        completed += 1
        if max_cells is not None and completed >= max_cells:
            return
    require(len(progress["records"]) == len(SEEDS), "target seed run incomplete")
    progress["status"] = "complete"
    write(root / "target-seed-progress.json", progress)


def js_bits(left, right):
    mixture = [(a + b) * 0.5 for a, b in zip(left, right)]
    return 0.5 * sum(a * math.log2(a / m) for a, m in zip(left, mixture) if a > 0) \
        + 0.5 * sum(b * math.log2(b / m) for b, m in zip(right, mixture) if b > 0)


def entropy(values):
    return -sum(value * math.log(value) for value in values if value > 0)


def summarize(root):
    protocol = read(root / "target-seed-protocol.json")
    progress = read(root / "target-seed-progress.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == len(SEEDS),
            "target seed run incomplete")
    by_seed = {}
    for seed in SEEDS:
        path = root / "raw/target-seed" / str(seed) / "first-root.json"
        record = next(row for row in progress["records"] if row["seed"] == seed)
        require(sha(path) == record["report_sha256"],
                f"target seed report changed: {seed}")
        by_seed[seed] = {case["id"]: case["mcts"]["root_records"][0]
                         for case in read(path)["cases"]}
    manifest = read(root / "continuation-actions.json")
    candidate_hashes = {case["case_id"]:
        [candidate["action_sha256"] for candidate in case["candidates"]]
        for case in manifest["cases"]}
    baseline = {}
    for stream in STREAMS:
        path = root / "raw" / stream / "c16-n128.json"
        require(sha(path) == protocol["baseline_report_sha256"][stream],
                f"baseline changed: {stream}")
        baseline.update({case["id"]: case["mcts"]["root_records"][0]
                         for case in read(path)["cases"]})
    require(len(baseline) == 48 and all(set(by_seed[seed]) == set(baseline)
                                        for seed in SEEDS), "case sets differ")
    cases = []
    variant_js = {name: [] for name in VARIANTS}
    variant_unvisited = {name: [] for name in VARIANTS}
    variant_effective = {name: [] for name in VARIANTS}
    rank_agreement = {name: [0, 0, 0] for name in VARIANTS}
    visit_jaccards = []
    selected_changes = 0
    selected_outside_original = 0
    for case_id in sorted(baseline):
        records = [by_seed[seed][case_id] for seed in SEEDS]
        original = baseline[case_id]
        witnesses = [action_key(action) for action in original["actions"]]
        require(len(set(witnesses)) == len(witnesses),
                f"duplicate witness: {case_id}")
        require(all(record["state_sha256"] == original["state_sha256"]
                    and [action_key(action) for action in record["actions"]] == witnesses
                    for record in records), f"root state or witnesses differ: {case_id}")
        index = {hashlib.sha256(key.encode()).hexdigest(): i
                 for i, key in enumerate(witnesses)}
        tested = [index[key] for key in candidate_hashes[case_id]]
        root_data = {}
        for seed, record in zip(SEEDS, records):
            prior, visited, distributions = targets(record["actions"])
            root_data[seed] = {"visited": {i for i, flag in enumerate(visited) if flag},
                               "selected": record["selected_action"],
                               "distributions": distributions}
            for name in VARIANTS:
                values = distributions[name]
                variant_unvisited[name].append(sum(value for value, flag in zip(values, visited)
                                                   if not flag))
                variant_effective[name].append(math.exp(entropy(values)) / len(values))
        baseline_visited = {i for i, action in enumerate(original["actions"])
                            if action["visits"] > 0}
        selected_hashes = [hashlib.sha256(witnesses[root_data[seed]["selected"]].encode()).hexdigest()
                           for seed in SEEDS]
        selected_changes += len(set(selected_hashes)) > 1
        selected_outside_original += sum(root_data[seed]["selected"] not in baseline_visited
                                         for seed in SEEDS)
        pair_rows = []
        for left_seed, right_seed in itertools.combinations(SEEDS, 2):
            left, right = root_data[left_seed], root_data[right_seed]
            overlap = len(left["visited"] & right["visited"])
            union = len(left["visited"] | right["visited"])
            visit_jaccards.append(overlap / union)
            js = {}
            for name in VARIANTS:
                a, b = left["distributions"][name], right["distributions"][name]
                divergence = js_bits(a, b)
                variant_js[name].append(divergence)
                js[name] = divergence
                for i, j in itertools.combinations(tested, 2):
                    sign_a = (a[i] > a[j]) - (a[i] < a[j])
                    sign_b = (b[i] > b[j]) - (b[i] < b[j])
                    if sign_a == 0 or sign_b == 0:
                        rank_agreement[name][2] += 1
                    elif sign_a == sign_b:
                        rank_agreement[name][0] += 1
                    else:
                        rank_agreement[name][1] += 1
            pair_rows.append({"seeds": [left_seed, right_seed],
                              "visited_jaccard": overlap / union,
                              "target_js_bits": js})
        cases.append({"case_id": case_id, "actions": len(witnesses),
                      "selected_sha256": selected_hashes,
                      "pair_comparisons": pair_rows})
    output = {"schema": "aic-hierarchical-target-seed-summary-v1",
              "protocol_sha256": sha(root / "target-seed-protocol.json"),
              "progress_sha256": sha(root / "target-seed-progress.json"),
              "cases": cases,
              "selected_changes_cases": selected_changes,
              "selected_outside_original_16": selected_outside_original,
              "mean_visited_jaccard": statistics.mean(visit_jaccards),
              "variants": {name: {
                  "mean_seed_pair_js_bits": statistics.mean(variant_js[name]),
                  "mean_unvisited_mass": statistics.mean(variant_unvisited[name]),
                  "mean_effective_fraction": statistics.mean(variant_effective[name]),
                  "tested_action_pair_rank_agreement": {
                      "concordant": rank_agreement[name][0],
                      "discordant": rank_agreement[name][1],
                      "tied": rank_agreement[name][2]}}
                  for name in VARIANTS}}
    write(root / "target-seed-summary.json", output)
    print({"selected_changes_cases": selected_changes,
           "selected_outside_original_16": selected_outside_original,
           "mean_visited_jaccard": output["mean_visited_jaccard"],
           "variants": output["variants"]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("write-configs", "prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, default=Path("artifacts/target-causal-study"))
    parser.add_argument("--experiment-commit")
    parser.add_argument("--max-cells", type=int)
    args = parser.parse_args()
    if args.operation == "write-configs":
        write_configs()
    elif args.operation == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    elif args.operation == "run":
        run(args.output, args.max_cells)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
