"""Force recorded first placements, then run paired downstream searches."""

import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time

from hierarchical_candidate_width import ARMS, CONFIG_ROOT, STREAMS, action_key, label
from ppo_harness_state import read, sha, write


SEEDS = (20260922, 20260923, 20260924)


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-first-action: {message}")


def search_config(seed):
    return CONFIG_ROOT / f"continuation-{seed}-search.json"


def outer_config(seed):
    return CONFIG_ROOT / f"continuation-{seed}-outer.json"


def model(root, seed):
    return root / "inputs" / "models" / f"outer-continuation-{seed}"


def write_configs():
    search = read(CONFIG_ROOT / "c16-n128-search.json")
    outer = read(CONFIG_ROOT / "c16-n128-outer.json")
    require(outer["search"] == search, "base configs differ")
    for seed in SEEDS:
        trial = json.loads(json.dumps(search))
        trial["max_moves"] = 23
        trial["root_selection"]["seed"] = seed
        write(search_config(seed), trial)
        write(outer_config(seed), dict(outer, search=trial))
    print(f"written_continuation_configs={len(SEEDS)}")


def candidate_manifest(root):
    stage1 = read(root / "summary.json")
    progress = read(root / "progress.json")
    require(stage1["progress_sha256"] == sha(root / "progress.json")
            and progress["status"] == "complete", "candidate-width summary is not final")
    results = []
    original_cases = {}
    for stream in STREAMS:
        panel = read(root / "inputs" / "panels" / f"{stream}.json")
        for case in panel["cases"]:
            require(case["id"] not in original_cases,
                    f"duplicate case ID across streams: {case['id']}")
            original_cases[case["id"]] = (stream, case)
        reports = {}
        for candidates, simulations in ARMS:
            arm = label(candidates, simulations)
            path = root / "raw" / stream / f"{arm}.json"
            record = next(row for row in progress["records"]
                          if row["stream"] == stream and row["arm"] == arm)
            require(sha(path) == record["report_sha256"],
                    f"candidate-width report changed: {stream}/{arm}")
            reports[arm] = read(path)
        for index, baseline_case in enumerate(reports["c16-n128"]["cases"]):
            case_id = baseline_case["id"]
            require(case_id in original_cases and original_cases[case_id][0] == stream,
                    f"source panel case missing: {case_id}")
            first = baseline_case["mcts"]["root_records"][0]
            actions = {action_key(action): action for action in first["actions"]}
            require(len(actions) == len(first["actions"]),
                    f"duplicate first-root witness: {case_id}")
            selected = []
            reasons = {}

            def add(key, reason):
                if key not in reasons:
                    selected.append(key)
                    reasons[key] = []
                reasons[key].append(reason)

            for candidates, simulations in ARMS:
                arm = label(candidates, simulations)
                other = reports[arm]["cases"][index]
                require(other["id"] == case_id, f"case order differs: {stream}/{arm}")
                root_record = other["mcts"]["root_records"][0]
                require(root_record["state_sha256"] == first["state_sha256"],
                        f"first state differs: {case_id}/{arm}")
                add(action_key(root_record["actions"][root_record["selected_action"]]),
                    f"selected-{arm}")
            visited = [(key, action) for key, action in actions.items()
                       if action["visits"] > 0]
            outside = [(key, action) for key, action in actions.items()
                       if action["visits"] == 0]
            require(len(visited) == 16 and outside,
                    f"unexpected baseline coverage: {case_id}")
            add(max(visited, key=lambda item: (item[1]["q"], item[0]))[0], "highest-q-16")
            add(min(visited, key=lambda item: (item[1]["q"], item[0]))[0], "lowest-q-16")
            anchor_eight = [
                (action_key(action), action)
                for _, action in sorted(enumerate(first["actions"]),
                                        key=lambda pair: (-pair[1]["anchor_prior"], pair[0]))[:8]
            ]
            require(all(action["visits"] > 0 for _, action in anchor_eight),
                    f"guaranteed anchor action was not visited: {case_id}")
            add(max(anchor_eight, key=lambda item: (item[1]["q"], item[0]))[0],
                "highest-q-guaranteed-anchor")
            add(max(outside, key=lambda item: (item[1]["anchor_prior"], item[0]))[0],
                "highest-anchor-outside-16")
            add(max(outside, key=lambda item:
                    (item[1]["prior"] + item[1]["anchor_prior"], item[0]))[0],
                "highest-mixed-prior-outside-16")
            wide_record = reports["c128-n512"]["cases"][index]["mcts"]["root_records"][0]
            wide_actions = {action_key(action): action for action in wide_record["actions"]}
            wide_outside = [(key, wide_actions[key]) for key, _ in outside
                            if wide_actions[key]["visits"] > 0]
            require(wide_outside, f"wide cell visited no outside actions: {case_id}")
            add(max(wide_outside, key=lambda item: (item[1]["q"], item[0]))[0],
                "highest-q-outside-16-in-c128-n512")
            add(min(outside, key=lambda item: hashlib.sha256(
                (case_id + "\0" + item[0]).encode()).digest())[0],
                "hash-sampled-outside-16")
            baseline_selected = action_key(first["actions"][first["selected_action"]])
            next_root = baseline_case["mcts"]["root_records"][1:2]
            arm_actions = {}
            for candidates, simulations in ARMS:
                arm = label(candidates, simulations)
                root_record = reports[arm]["cases"][index]["mcts"]["root_records"][0]
                arm_actions[arm] = {action_key(action): action
                                    for action in root_record["actions"]}
            results.append({
                "stream": stream, "case_id": case_id,
                "baseline_first_state_sha256": first["state_sha256"],
                "baseline_next_state_sha256": next_root[0]["state_sha256"]
                if next_root else None,
                "baseline_selected_sha256": hashlib.sha256(
                    baseline_selected.encode()).hexdigest(),
                "candidates": [
                    {"action_sha256": hashlib.sha256(key.encode()).hexdigest(),
                     "action": actions[key]["action"], "reasons": reasons[key],
                     "baseline_visits": actions[key]["visits"],
                     "baseline_q": actions[key]["q"],
                     "baseline_policy_target": actions[key]["policy_target"],
                     "baseline_prior": actions[key]["prior"],
                     "baseline_anchor_prior": actions[key]["anchor_prior"],
                     "search_evidence": {
                         arm: {"visits": witnesses[key]["visits"],
                               "q": witnesses[key]["q"],
                               "policy_target": witnesses[key]["policy_target"]}
                         for arm, witnesses in arm_actions.items()
                     }}
                    for key in selected
                ],
            })
    require(len(results) == 48, "wrong case count")
    return results, original_cases


def prepare(root, commit):
    require(not (root / "continuation-protocol.json").exists(),
            "continuation already prepared")
    candidates, originals = candidate_manifest(root)
    maximum = max(len(case["candidates"]) for case in candidates)
    panels = {}
    for rank in range(maximum):
        cases = []
        for row in candidates:
            if rank >= len(row["candidates"]):
                continue
            witness = row["candidates"][rank]["action"]
            require(witness["kind"] == "command", "non-command first macro needs native replay")
            original = originals[row["case_id"]][1]
            cases.append(dict(original, prefix=[*original["prefix"], witness["value"]]))
        require(cases, f"empty continuation rank {rank}")
        template = read(root / "inputs" / "panels" / "s1.json")
        panel = {"schema": template["schema"], "partition": "training",
                 "limits": template["limits"], "cases": cases}
        path = root / "inputs" / "continuation-panels" / f"rank-{rank}.json"
        path.parent.mkdir(parents=True, exist_ok=True)
        write(path, panel)
        panels[str(rank)] = {"sha256": sha(path), "cases": len(cases)}
    write(root / "continuation-actions.json", {
        "schema": "aic-hierarchical-first-action-candidates-v1",
        "candidate_width_summary_sha256": sha(root / "summary.json"),
        "cases": candidates,
    })
    config_hashes = {}
    for seed in SEEDS:
        search = read(search_config(seed))
        outer = read(outer_config(seed))
        require(search["max_moves"] == 23
                and search["root_selection"]["seed"] == seed
                and search["root_selection"]["candidates"] == 16
                and search["simulations_per_move"] == 128
                and outer["search"] == search,
                f"continuation config invalid: {seed}")
        directory = model(root, seed)
        directory.mkdir(exist_ok=True)
        for name in ("state.safetensors", "parameter-order.json"):
            link = directory / name
            if not link.exists():
                link.symlink_to(Path("..") / "outer-128" / name)
        write(directory / "config.json", outer)
        config_hashes[str(seed)] = {"search": sha(search_config(seed)),
                                    "outer": sha(outer_config(seed)),
                                    "model_config": sha(directory / "config.json")}
    protocol = {
        "schema": "aic-hierarchical-first-action-protocol-v1",
        "experiment_commit": commit,
        "candidate_width_protocol_sha256": sha(root / "protocol.json"),
        "candidate_width_summary_sha256": sha(root / "summary.json"),
        "actions_sha256": sha(root / "continuation-actions.json"),
        "panels": panels, "configs": config_hashes,
        "binary_sha256": sha(Path("target/release/aic-cli")),
        "lower_checkpoint_sha256": sha(root / "inputs/models/lower/state.safetensors"),
        "outer_checkpoint_sha256": sha(root / "inputs/models/outer-128/state.safetensors"),
        "planned_cells": len(SEEDS) * len(panels),
    }
    write(root / "continuation-protocol.json", protocol)
    write(root / "continuation-progress.json", {
        "schema": "aic-hierarchical-first-action-progress-v1",
        "protocol_sha256": sha(root / "continuation-protocol.json"),
        "records": [], "status": "prepared"})
    print(f"prepared_candidates=48 max_rank={maximum} cells={protocol['planned_cells']}")


def run(root, max_cells):
    protocol = read(root / "continuation-protocol.json")
    progress = read(root / "continuation-progress.json")
    require(progress["protocol_sha256"] == sha(root / "continuation-protocol.json"),
            "continuation protocol changed")
    require(sha(Path("target/release/aic-cli")) == protocol["binary_sha256"]
            and sha(root / "continuation-actions.json") == protocol["actions_sha256"],
            "binary or action witness changed")
    for rank, info in protocol["panels"].items():
        require(sha(root / "inputs/continuation-panels" / f"rank-{rank}.json")
                == info["sha256"], f"panel changed: {rank}")
    for seed in SEEDS:
        hashes = protocol["configs"][str(seed)]
        require(sha(search_config(seed)) == hashes["search"]
                and sha(outer_config(seed)) == hashes["outer"]
                and sha(model(root, seed) / "config.json") == hashes["model_config"]
                and sha(model(root, seed) / "state.safetensors")
                == protocol["outer_checkpoint_sha256"],
                f"search or model changed: {seed}")
    progress["status"] = "running"
    write(root / "continuation-progress.json", progress)
    cells = 0
    for seed in SEEDS:
        for rank in protocol["panels"]:
            if any(record["seed"] == seed and record["rank"] == int(rank)
                   for record in progress["records"]):
                continue
            output = root / "raw" / "continuation" / str(seed) / f"rank-{rank}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            log = root / "logs" / f"continuation-{seed}-rank-{rank}.log"
            started = time.monotonic()
            if not output.exists():
                command = ["target/release/aic-cli", "learning", "hierarchical-search-harness",
                           "--config", str(search_config(seed)),
                           "--panel", str(root / "inputs/continuation-panels" / f"rank-{rank}.json"),
                           "--lower-training-output", str(root / "inputs/models/lower"),
                           "--outer-training-output", str(model(root, seed)),
                           "--catalog-root", "data/game/normalized", "--output", str(output),
                           "--device", "cuda:0"]
                with log.open("w") as stream:
                    result = subprocess.run(command, stdout=stream,
                                            stderr=subprocess.STDOUT, check=False)
                require(result.returncode == 0, f"continuation failed: {seed}/{rank}")
            report = read(output)
            require(report["status"] == "complete" and report["partition"] == "training"
                    and report["panel_sha256"] == protocol["panels"][rank]["sha256"]
                    and report["lower_checkpoint_sha256"]
                    == protocol["lower_checkpoint_sha256"]
                    and report["outer_checkpoint_sha256"]
                    == protocol["outer_checkpoint_sha256"]
                    and len(report["cases"]) == protocol["panels"][rank]["cases"],
                    f"invalid continuation report: {seed}/{rank}")
            progress["records"].append({"seed": seed, "rank": int(rank),
                                        "report_sha256": sha(output),
                                        "report_bytes": output.stat().st_size,
                                        "wall_seconds": time.monotonic() - started})
            write(root / "continuation-progress.json", progress)
            print(f"completed_continuation={seed}/{rank}", flush=True)
            cells += 1
            if max_cells is not None and cells >= max_cells:
                return
    progress["status"] = "complete"
    write(root / "continuation-progress.json", progress)


def summarize(root):
    protocol = read(root / "continuation-protocol.json")
    progress = read(root / "continuation-progress.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_cells"],
            "continuation matrix incomplete")
    actions = read(root / "continuation-actions.json")["cases"]
    by_id = {case["case_id"]: case for case in actions}
    require(len(by_id) == 48, "candidate case identities are not unique")
    outcome = {case_id: [None] * len(case["candidates"])
               for case_id, case in by_id.items()}
    for seed in SEEDS:
        for rank in protocol["panels"]:
            path = root / "raw/continuation" / str(seed) / f"rank-{rank}.json"
            record = next(row for row in progress["records"]
                          if row["seed"] == seed and row["rank"] == int(rank))
            require(sha(path) == record["report_sha256"],
                    f"continuation raw changed: {seed}/{rank}")
            report = read(path)
            for case in report["cases"]:
                case_id, result = case["id"], case["mcts"]
                require(case_id in by_id and int(rank) < len(by_id[case_id]["candidates"]),
                        f"unexpected continuation case: {seed}/{rank}/{case_id}")
                if outcome[case_id][int(rank)] is None:
                    outcome[case_id][int(rank)] = []
                first = result["root_records"][:1]
                state = first[0]["state_sha256"] if first else None
                original = by_id[case_id]
                if (original["candidates"][int(rank)]["action_sha256"]
                        == original["baseline_selected_sha256"]
                        and original["baseline_next_state_sha256"] is not None):
                    require(state == original["baseline_next_state_sha256"],
                            f"forced baseline state differs: {case_id}/{seed}")
                outcome[case_id][int(rank)].append({
                    "seed": seed, "first_state_sha256": state,
                    "completed": result["fully_validated"],
                    "completed_connections": result["completed_connections"],
                    "total_connections": result["total_connections"],
                    "footprint_area": result["footprint_area"],
                    "transport_cells": result["transport_cells"],
                    "turns": result["turns"],
                    "elapsed_seconds": result["elapsed_seconds"],
                })
    compact = []
    for case in actions:
        variants = []
        for rank, candidate in enumerate(case["candidates"]):
            trials = outcome[case["case_id"]][rank]
            require(trials is not None and len(trials) == len(SEEDS),
                    f"missing continuation: {case['case_id']}/{rank}")
            states = {trial["first_state_sha256"] for trial in trials}
            require(len(states) == 1,
                    f"forced state varies by seed: {case['case_id']}/{rank}")
            variants.append({
                "rank": rank, "action_sha256": candidate["action_sha256"],
                "reasons": candidate["reasons"],
                "baseline_visits": candidate["baseline_visits"],
                "baseline_q": candidate["baseline_q"],
                "search_evidence": candidate["search_evidence"],
                "completion_count": sum(trial["completed"] for trial in trials),
                "mean_completed_connections": statistics.mean(
                    trial["completed_connections"] for trial in trials),
                "trials": trials,
            })
        compact.append({"stream": case["stream"], "case_id": case["case_id"],
                        "variants": variants})
    q_ranking = {}
    for candidates, simulations in ARMS:
        arm = label(candidates, simulations)
        pairs = 0
        concordant = 0
        tied = 0
        outside_winners = 0
        cases_with_pairs = 0
        for case in compact:
            variants = case["variants"]
            count_here = 0
            for left in range(len(variants)):
                for right in range(left + 1, len(variants)):
                    difference = (variants[left]["completion_count"]
                                  - variants[right]["completion_count"])
                    if abs(difference) < 2:
                        continue
                    winner, loser = (variants[left], variants[right]) if difference > 0 \
                        else (variants[right], variants[left])
                    wq, lq = winner["search_evidence"][arm], loser["search_evidence"][arm]
                    if wq["visits"] == 0 or lq["visits"] == 0:
                        continue
                    pairs += 1
                    count_here += 1
                    concordant += wq["q"] > lq["q"]
                    tied += wq["q"] == lq["q"]
                    outside_winners += winner["baseline_visits"] == 0
            cases_with_pairs += count_here > 0
        q_ranking[arm] = {"clear_pairs_both_visited": pairs,
                          "cases_with_pairs": cases_with_pairs,
                          "q_concordant": concordant, "q_tied": tied,
                          "outside_baseline_winners": outside_winners}
    write(root / "continuation-summary.json", {
        "schema": "aic-hierarchical-first-action-summary-v1",
        "protocol_sha256": sha(root / "continuation-protocol.json"),
        "progress_sha256": sha(root / "continuation-progress.json"),
        "cases": compact, "q_ranking": q_ranking,
    })
    print(f"summarized_cases={len(compact)} variants={sum(len(c['variants']) for c in compact)}")
    print(json.dumps(q_ranking, indent=2))


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
        require(args.experiment_commit is not None, "--experiment-commit required")
        prepare(args.output, args.experiment_commit)
    elif args.operation == "run":
        run(args.output, args.max_cells)
    else:
        summarize(args.output)


if __name__ == "__main__":
    main()
