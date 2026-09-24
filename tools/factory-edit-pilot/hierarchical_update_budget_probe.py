"""Run paired outer-policy epoch prefixes over one frozen collection per stream."""

import argparse
import json
import os
from pathlib import Path
import subprocess

from ppo_harness_state import read, sha, write


STREAMS = ("s1", "s2", "s3")
BUDGETS = (25, 50, 100, 200)
LOWER_SHA = "353d560842da6c4270605da552a04adc8d96411512d8d3b847625cf9c502dea9"
OUTER_SHA = "6a1f61578aa29a9e80d90ab7805bfec2ef8e543bd24fd9db90441fbb5d724520"
BINARY_SHA = "e594b5425074a03731524d8c91991126232d840ef44a86e8b0630e1f16800fdf"


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-update-budget: {message}")


def paths(root):
    inputs = root / "inputs"
    configs = Path("docs/benchmarks/hierarchical-update-budget")
    search = Path("docs/benchmarks/hierarchical-policy-stability/search.json")
    return inputs, configs, search


def prepare(root, experiment_commit):
    inputs, configs, search = paths(root)
    require(inputs.is_dir() and not (root / "protocol.json").exists(), "inputs or protocol state differs")
    diagnostic = read(inputs / "diagnostic.json")
    training = {stream: read(inputs / "training" / f"{stream}.json") for stream in STREAMS}
    require(diagnostic["partition"] == "training" and len(diagnostic["cases"]) == 64,
            "diagnostic panel must have 64 train-source cases")
    for stream, panel in training.items():
        require(panel["partition"] == "training" and len(panel["cases"]) == 32,
                f"wrong training panel {stream}")
    def identity(case):
        import hashlib
        encoded = json.dumps(case["problem"], sort_keys=True, separators=(",", ":"))
        return hashlib.sha256(encoded.encode()).hexdigest()
    diagnostic_ids = [identity(case) for case in diagnostic["cases"]]
    training_ids = {identity(case) for panel in training.values() for case in panel["cases"]}
    prior_ids = {identity(case) for name in ("prior-screen-diagnostic", "prior-confirmation-diagnostic")
                 for case in read(inputs / f"{name}.json")["cases"]}
    require(len(set(diagnostic_ids)) == 64 and not (set(diagnostic_ids) & (training_ids | prior_ids)),
            "diagnostic identities overlap training or prior diagnostics")
    require(sha(inputs / "initial-lower/state.safetensors") == LOWER_SHA
            and sha(inputs / "initial-outer/state.safetensors") == OUTER_SHA,
            "initial checkpoint identity differs")
    require(sha(Path("target/release/aic-cli")) == BINARY_SHA, "release binary identity differs")
    config_hashes = {}
    for budget in BUDGETS:
        path = configs / f"generation-{budget}.json"
        config = read(path)
        require(config["epochs"] == budget and config["outer_states_per_epoch"] == 256
                and config["batch_states"] == 16 and config["seed"] == 20260925
                and config["outer_learning_rate"] == 0.0003
                and not config["train_lower"] and config["train_failed_policy"],
                f"training config changed: {path}")
        config_hashes[str(budget)] = sha(path)
    protocol = {
        "schema": "aic-hierarchical-update-budget-probe-v1",
        "training_source_commit": "31555f5",
        "experiment_commit": experiment_commit,
        "server_root": "~/Srv/aic-policy-stability-20260923",
        "release_binary_sha256": BINARY_SHA,
        "search_config_sha256": sha(search),
        "sampling_sha256": sha(Path("docs/benchmarks/direct-constructive-full-graphs/sampling.json")),
        "generation_config_sha256": config_hashes,
        "initial_lower_sha256": LOWER_SHA,
        "initial_outer_sha256": OUTER_SHA,
        "training_panel_sha256": {stream: sha(inputs / "training" / f"{stream}.json") for stream in STREAMS},
        "diagnostic_panel_sha256": sha(inputs / "diagnostic.json"),
        "diagnostic_candidates_sha256": sha(inputs / "diagnostic-candidates.json"),
        "prior_screen_diagnostic_sha256": sha(inputs / "prior-screen-diagnostic.json"),
        "prior_confirmation_diagnostic_sha256": sha(inputs / "prior-confirmation-diagnostic.json"),
        "training_problem_identities": len(training_ids),
        "diagnostic_cases": 64,
        "planned_records": 12,
        "budgets_epochs": list(BUDGETS),
        "streams": list(STREAMS),
    }
    write(root / "protocol.json", protocol)
    write(root / "progress.json", {
        "schema": "aic-hierarchical-update-budget-progress-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "status": "prepared",
        "records": [],
    })
    print(f"prepared_protocol_sha256={sha(root / 'protocol.json')}", flush=True)


def cli(root, label, args):
    log = root / "logs" / f"{label}.log"
    log.parent.mkdir(exist_ok=True)
    with log.open("w") as output:
        result = subprocess.run(["target/release/aic-cli", "learning", *args],
                                stdout=output, stderr=subprocess.STDOUT, check=False)
    require(result.returncode == 0, f"{label} failed ({result.returncode}); see {log}")


def outcome(path, case_order):
    report = read(path)
    require(report["partition"] == "training" and len(report["cases"]) == 64,
            f"wrong evaluation report: {path}")
    by_id = {case["id"]: bool(case["mcts"]["fully_validated"])
             for case in report["cases"]}
    require(set(by_id) == set(case_order), f"evaluation case identity changed: {path}")
    return [by_id[case_id] for case_id in case_order]


def run(root):
    inputs, configs, search = paths(root)
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    observer = read(root / "wandb-observer.json")
    require(protocol["schema"] == "aic-hierarchical-update-budget-probe-v1"
            and progress["protocol_sha256"] == sha(root / "protocol.json"),
            "protocol changed")
    require(observer["status"] in ("ready", "running")
            and observer["remote_confirmed_step"] >= 0,
            "W&B readiness record is not acknowledged")
    require(sha(Path("target/release/aic-cli")) == protocol["release_binary_sha256"],
            "release binary changed")
    require(sha(search) == protocol["search_config_sha256"]
            and sha(inputs / "diagnostic.json") == protocol["diagnostic_panel_sha256"]
            and sha(inputs / "initial-lower/state.safetensors") == protocol["initial_lower_sha256"]
            and sha(inputs / "initial-outer/state.safetensors") == protocol["initial_outer_sha256"],
            "stable diagnostic, search or starting-model input changed")
    for stream in STREAMS:
        require(sha(inputs / "training" / f"{stream}.json")
                == protocol["training_panel_sha256"][stream],
                f"training panel changed: {stream}")
    for budget in BUDGETS:
        require(sha(configs / f"generation-{budget}.json")
                == protocol["generation_config_sha256"][str(budget)],
                f"generation config changed: {budget}")
    panel = read(inputs / "diagnostic.json")
    case_order = [case["id"] for case in panel["cases"]]
    easy = [len(case["problem"]["connections"]) <= 4 for case in panel["cases"]]
    lower = inputs / "initial-lower"
    outer = inputs / "initial-outer"
    catalog = "data/game/normalized"
    baseline_path = root / "baseline-raw.json"
    if not baseline_path.exists():
        cli(root, "baseline", ["hierarchical-search-harness", "--config", str(search),
            "--panel", str(inputs / "diagnostic.json"),
            "--lower-training-output", str(lower), "--outer-training-output", str(outer),
            "--catalog-root", catalog, "--output", str(baseline_path), "--device", "cuda:0"])
    baseline = outcome(baseline_path, case_order)
    write(root / "baseline.json", {
        "schema": "aic-hierarchical-update-budget-baseline-v1",
        "diagnostic_panel_sha256": protocol["diagnostic_panel_sha256"],
        "raw_report_sha256": sha(baseline_path),
        "case_order": case_order,
        "case_success": "".join("1" if value else "0" for value in baseline),
        "completed": sum(baseline),
        "easy_completed": sum(value and in_easy for value, in_easy in zip(baseline, easy)),
    })
    progress["status"] = "running"
    progress["baseline_sha256"] = sha(root / "baseline.json")
    write(root / "progress.json", progress)

    for stream in STREAMS:
        training_panel = inputs / "training" / f"{stream}.json"
        collection = root / "collections" / f"{stream}.json"
        collection.parent.mkdir(exist_ok=True)
        if not collection.exists():
            cli(root, f"collect-{stream}", ["hierarchical-search-harness", "--config", str(search),
                "--panel", str(training_panel), "--lower-training-output", str(lower),
                "--outer-training-output", str(outer), "--catalog-root", catalog,
                "--output", str(collection), "--device", "cuda:0"])
        collected = read(collection)
        require(collected["root_records_retained"] and collected["partition"] == "training"
                and len(collected["cases"]) == 32
                and collected["panel_sha256"] == protocol["training_panel_sha256"][stream]
                and collected["lower_checkpoint_sha256"] == LOWER_SHA
                and collected["outer_checkpoint_sha256"] == OUTER_SHA,
                f"frozen collection identity changed: {stream}")
        del collected
        collection_sha = sha(collection)

        for budget in BUDGETS:
            label = f"{stream}-{budget}"
            if any(row["stream"] == stream and row["epochs"] == budget
                   for row in progress["records"]):
                continue
            output = root / "training" / stream / str(budget)
            output.parent.mkdir(parents=True, exist_ok=True)
            if not output.exists():
                pending = output.with_name(f".{output.name}.pending")
                arguments = ["train-hierarchical-generation", "--config",
                    str(configs / f"generation-{budget}.json"), "--panel", str(training_panel),
                    "--collection", str(collection), "--lower-training-output", str(lower),
                    "--outer-training-output", str(outer), "--catalog-root", catalog,
                    "--output", str(output), "--device", "cuda:0"]
                if pending.exists():
                    arguments.append("--resume")
                cli(root, f"train-{label}", arguments)
            training_report_path = output / "report.json"
            trained = read(training_report_path)
            require(trained["status"] == "complete" and trained["epochs"] == budget
                    and trained["updates"] == 16 * budget
                    and trained["outer_states_per_epoch"] == 256
                    and trained["collection_sha256"] == collection_sha
                    and trained["lower_checkpoint_sha256"] == LOWER_SHA
                    and trained["source_outer_checkpoint_sha256"] == OUTER_SHA
                    and not trained["train_lower"] and trained["train_failed_policy"],
                    f"training budget or input changed: {label}")
            evaluation = root / "evaluations" / stream / f"{budget}.json"
            evaluation.parent.mkdir(parents=True, exist_ok=True)
            if not evaluation.exists():
                cli(root, f"evaluate-{label}", ["hierarchical-search-harness", "--config", str(search),
                    "--panel", str(inputs / "diagnostic.json"),
                    "--lower-training-output", str(output / "lower"),
                    "--outer-training-output", str(output / "outer"),
                    "--catalog-root", catalog, "--output", str(evaluation), "--device", "cuda:0"])
            result = outcome(evaluation, case_order)
            row = {
                "stream": stream,
                "epochs": budget,
                "updates": trained["updates"],
                "completed": sum(result),
                "gained_vs_start": sum(now and not start for now, start in zip(result, baseline)),
                "lost_vs_start": sum(start and not now for now, start in zip(result, baseline)),
                "easy_completed": sum(value and in_easy for value, in_easy in zip(result, easy)),
                "case_success": "".join("1" if value else "0" for value in result),
                "collection_sha256": collection_sha,
                "training_report_sha256": sha(training_report_path),
                "evaluation_report_sha256": sha(evaluation),
                "outer_checkpoint_sha256": trained["outer_checkpoint_sha256"],
                "initial_outer_loss": trained["initial_outer_loss"],
                "final_outer_loss": trained["final_outer_loss"],
                "training_seconds": trained["elapsed_seconds"],
            }
            progress["records"].append(row)
            write(root / "progress.json", progress)
            print(f"completed={label} diagnostic={row['completed']}/64 "
                  f"lost_from_start={row['lost_vs_start']}", flush=True)
    require(len(progress["records"]) == 12, "matrix did not finish all budgets")
    progress["status"] = "complete"
    write(root / "progress.json", progress)
    print("matrix_complete=True", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--experiment-commit")
    args = parser.parse_args()
    os.chdir(Path(__file__).resolve().parents[2])
    if args.command == "prepare":
        require(args.experiment_commit, "experiment commit is required")
        prepare(args.output, args.experiment_commit)
    else:
        run(args.output)


if __name__ == "__main__":
    main()
