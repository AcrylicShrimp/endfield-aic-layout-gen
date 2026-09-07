"""Bounded equal-action horizon pilot; existing learner and environment stay unchanged."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate(plan):
    if plan.get("schema_version") != 1 or plan.get("horizons") != [16, 32, 64]:
        raise ValueError("Expected pilot schema 1 and fixed horizons [16, 32, 64]")
    for key, maximum in (("workers", 16), ("threads", 16), ("training_actions_per_arm", 65536),
                         ("training_timeout_seconds", 1800), ("evaluation_timeout_seconds", 1800)):
        if type(plan.get(key)) is not int or not 1 <= plan[key] <= maximum:
            raise ValueError(f"Invalid {key}")
    if plan["workers"] < 2 or type(plan.get("seed")) is not int or plan.get("zero_control_horizon") != 16:
        raise ValueError("Expected multiple workers, an integer seed and H16 zero control")
    if any(plan["training_actions_per_arm"] % (h * plan["workers"]) for h in plan["horizons"]):
        raise ValueError("Training action budget must divide every horizon/worker batch")
    if plan.get("evaluation_proposals") != [128, 512, 1024] or plan.get("rollout_seeds") != [101]:
        raise ValueError("Pilot validation matrix differs from the frozen design")


def execute(command, timeout):
    print(json.dumps(dict(stage="launch", command=command, timeout_seconds=timeout)), flush=True)
    child = subprocess.Popen(command, start_new_session=True)
    try:
        code = child.wait(timeout=timeout)
    except BaseException:
        # Kill only the process group created for this owned experiment subprocess.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        raise
    if code != 0:
        raise RuntimeError(f"Experiment subprocess exited {code}: {command}")


def run(options):
    plan_path = Path(options.plan).resolve()
    plan = read(plan_path)
    validate(plan)
    roots_path = (plan_path.parent / plan["roots"]).resolve()
    dataset = read(roots_path)
    corpus_report = read(roots_path.with_name("report.json"))
    if not dataset.get("factory_validated") or corpus_report.get("gate") is not True:
        raise ValueError(f"Corpus gate did not pass: {roots_path}")
    for p in dataset["provenance"]:
        if sha(p["path"]) != p["sha256"]:
            raise ValueError(f'Stale corpus input: {p["path"]}')
    output = Path(options.output_directory).resolve()
    output.mkdir()
    base = Path(__file__).resolve().parent.parent / "layout-rl"
    inputs = [plan_path, roots_path, roots_path.with_name("report.json"), Path(__file__).resolve(),
              Path(__file__).with_name("audit-pilot.mjs").resolve(), base / "train.py", base / "evaluate.py"]
    provenance = [dict(path=str(p), sha256=sha(p)) for p in inputs]
    write(output / "provenance.json", provenance)
    began = time.perf_counter()
    training_reports = []
    for horizon in plan["horizons"]:
        groups = plan["training_actions_per_arm"] // (plan["workers"] * horizon)
        runs = [dict(signal="terminal", seed=plan["seed"])]
        if horizon == plan["zero_control_horizon"]:
            runs.append(dict(signal="zero", seed=plan["seed"]))
        config = dict(schema_version=1, groups=groups, workers=plan["workers"], horizon=horizon,
                      threads=plan["threads"], width=32, architecture="graph", learning_rate=.001,
                      curve_interval=max(1, groups // 4), runs=runs)
        config_path = output / f"h{horizon}-training.json"
        write(config_path, config)
        directory = output / f"h{horizon}"
        execute([sys.executable, str(base / "train.py"), "--roots", str(roots_path),
                 "--config", str(config_path), "--output-directory", str(directory)], plan["training_timeout_seconds"])
        training_reports.append(str(directory / "report.json"))
    evaluation = dict(schema_version=1, threads=plan["threads"], training_reports=training_reports,
                      splits=["validation"], baselines=["retain", "uniform", "greedy", "best-first"],
                      decoders=["sample"], rollout_seeds=plan["rollout_seeds"], order_seed=918,
                      budgets=[dict(mode="work", proposals=p, elapsed_ms=30000) for p in plan["evaluation_proposals"]])
    evaluation_path = output / "evaluation.json"
    write(evaluation_path, evaluation)
    execute([sys.executable, str(base / "evaluate.py"), "--roots", str(roots_path),
             "--config", str(evaluation_path), "--output-directory", str(output / "evaluation")], plan["evaluation_timeout_seconds"])
    audit_plan = output / "audit-plan.json"
    write(audit_plan, dict(schema_version=1, roots=str(roots_path), corpus_report=str(roots_path.with_name("report.json")),
                          training_reports=training_reports, evaluation_report=str(output / "evaluation" / "report.json"),
                          training_actions_per_arm=plan["training_actions_per_arm"]))
    execute(["node", str(Path(__file__).with_name("audit-pilot.mjs")), "--plan", str(audit_plan),
             "--report-output", str(output / "audit.json")], 300)
    for p in provenance:
        if sha(p["path"]) != p["sha256"]:
            raise RuntimeError(f'Pilot input changed: {p["path"]}')
    write(output / "report.json", dict(schema_version=1, status="complete", scope="one-seed-validation-pilot",
                                      elapsed_seconds=time.perf_counter() - began, roots=str(roots_path),
                                      training_reports=training_reports, evaluation=str(output / "evaluation" / "report.json"),
                                      audit=str(output / "audit.json"), provenance=provenance))
    print(json.dumps(dict(status="complete", output=str(output), elapsed_seconds=time.perf_counter() - began)), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--plan", required=True)
    parser.add_argument("--output-directory", required=True)
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="rl-next-pilot-failed", severity="error", stage="pilot", message=str(error), entity=None)), flush=True)
        raise SystemExit(1)
