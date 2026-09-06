"""Matched-budget evaluation with fresh online routing and independent final audits."""

import argparse
import hashlib
import json
from pathlib import Path
import random
import time

import torch

from client import Worker
from policy import distribution, restore


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(path):
    return json.loads(Path(path).read_text())


def run(options):
    config_path = Path(options.config).resolve()
    config = read(config_path)
    if config.get("schema_version") != 1 or not config.get("budgets") or not config.get("rollout_seeds"):
        raise ValueError("Invalid evaluation config")
    torch.set_num_threads(config.get("threads", 1))
    torch.set_num_interop_threads(1)
    output = Path(options.output_directory).resolve()
    output.mkdir()
    roots_path = Path(options.roots).resolve()
    models, inputs = [], [roots_path, config_path, Path(__file__).resolve(), Path(__file__).with_name('client.py').resolve(), Path(__file__).with_name('policy.py').resolve()]
    for relative in config.get("training_reports", []):
        report_path = (config_path.parent / relative).resolve()
        report = read(report_path)
        inputs.append(report_path)
        if report.get("status") != "complete" or report["roots"] != str(roots_path):
            raise ValueError(f"Training report is incomplete or uses different roots: {report_path}")
        for source in report["provenance"]:
            if sha(source["path"]) != source["sha256"]:
                raise ValueError(f'Training source changed: {source["path"]}')
        for trained in report["runs"]:
            checkpoint = trained["checkpoint"]
            if sha(checkpoint["path"]) != checkpoint["sha256"]:
                raise ValueError(f'Model hash changed: {checkpoint["path"]}')
            model = restore(read(checkpoint["path"]))
            model.eval()
            inputs.append(Path(checkpoint["path"]))
            for decode in config.get("decoders", ["sample"]):
                if decode not in ("sample", "argmax"):
                    raise ValueError("Unknown policy decoder")
                models.append(dict(name=f'{report_path.parent.name}/{trained["name"]}/{decode}', model=model,
                                   signal=trained["signal"], model_seed=trained["seed"], decoder=decode, checkpoint=checkpoint))
    policies = [dict(name=name, model=None, signal=name, model_seed=None, decoder=None)
                for name in config.get("baselines", ["retain", "uniform", "greedy", "best-first"])] + models
    if any(p["model"] is None and p["name"] not in ("retain", "uniform", "greedy", "best-first") for p in policies):
        raise ValueError("Unknown baseline")
    provenance = [dict(path=str(p), sha256=sha(p)) for p in inputs]
    worker = Worker(roots_path, output / "worker", timeout=60)
    rows = []
    began = time.perf_counter()
    try:
        metadata = worker.call("hello")
        roots = [r for r in metadata["roots"] if r["split"] in config.get("splits", ["test", "out-of-family"])]
        if config.get("root_ids") is not None:
            roots = [r for r in metadata["roots"] if r["id"] in config["root_ids"]]
            if {r["id"] for r in roots} != set(config["root_ids"]):
                raise ValueError("Unknown explicit evaluation root")
        jobs = [(root, policy, seed, budget) for root in roots for policy in policies
                for budget in config["budgets"] for seed in config["rollout_seeds"]
                if not (budget["mode"] == "work" and (policy["name"] in ("retain", "greedy", "best-first") or policy["decoder"] == "argmax")
                        and seed != config["rollout_seeds"][0])]
        # Randomized execution order prevents always timing one method first or last.
        random.Random(config.get("order_seed", 918)).shuffle(jobs)
        for i, (root, policy, seed, budget) in enumerate(jobs):
            if budget["mode"] not in ("work", "time"):
                raise ValueError("Unknown budget mode")
            torch.manual_seed(seed)
            generator = random.Random(seed)
            observation = worker.call("reset", root=root["id"], budget=dict(steps=budget["proposals"], elapsed_ms=budget["elapsed_ms"]))
            inference_seconds = 0.0
            if policy["name"] in ("greedy", "best-first"):
                summary = worker.call("search", policy=dict(policy=policy["name"], beam_width=8, branch_limit=8))
            else:
                while policy["name"] != "retain" and not observation["done"]:
                    start = time.perf_counter()
                    if policy["model"] is None:
                        valid = [a for a, yes in enumerate(observation["features"]["action_mask"]) if yes]
                        chosen = generator.choice(valid)
                    else:
                        with torch.no_grad():
                            dist = distribution(policy["model"], [observation])
                            chosen = int(dist.sample()[0] if policy["decoder"] == "sample" else dist.logits.argmax(-1)[0])
                    inference_seconds += time.perf_counter() - start
                    observation = worker.call("step", action=chosen)
                summary = worker.call("summary")
            artifact = worker.call("save")
            audit = worker.call("audit")
            summary.update(root_metadata=root, policy={k: v for k, v in policy.items() if k != "model"},
                           rollout_seed=seed, budget=budget, inference_seconds=inference_seconds, artifact=artifact, audit=audit)
            if summary["work"]["proposals"] > budget["proposals"] or summary["work"]["validations"] > budget["proposals"]:
                raise RuntimeError("Evaluation exceeded matched proposal/validation cap")
            if not audit["coverage"]["factory_validated"]:
                raise RuntimeError("Fresh final audit rejected an incumbent")
            rows.append(summary)
            if (i + 1) % 16 == 0 or i + 1 == len(jobs):
                print(json.dumps(dict(completed=i + 1, total=len(jobs), elapsed_seconds=time.perf_counter() - began)), flush=True)
    finally:
        worker.close()
    for source in provenance:
        if sha(source["path"]) != source["sha256"]:
            raise RuntimeError(f'Evaluation input/source changed: {source["path"]}')
    result = dict(schema_version=1, status="complete", config=config, roots=str(roots_path), rows=rows,
                  elapsed_seconds=time.perf_counter() - began, provenance=provenance)
    with output.joinpath("report.json").open("x") as stream:
        json.dump(result, stream)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--roots", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--output-directory", required=True)
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="rl-evaluation-failed", severity="error", stage="evaluation", message=str(error), entity=None)), flush=True)
        raise SystemExit(1)
