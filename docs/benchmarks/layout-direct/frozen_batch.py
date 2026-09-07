"""Independent frozen-policy groups and prospective matched single-update controls."""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import time

import torch

from client import Pool
from gradient_audit import configure_runtime, cosine, raw_gradient, vector
from replay import (CAPSULE_BYTES, file_reference, referenced_json, restore_snapshot, snapshot,
                    pack_histories, save_capsule, verify)
from train import credit_signals, rollout
from update import apply_update, distribution_changes, fingerprint

ARMS = ("prefix", "terminal_same_outcomes", "shuffled", "zero")


def seed(role, root, batch, evaluation=0):
    value = ["frozen-batch-v1", role, root, batch, evaluation]
    return int.from_bytes(hashlib.sha256(json.dumps(value).encode()).digest()[:8], "little")


def signals_for(credit, histories):
    return dict(prefix=credit["signals"],
                terminal_same_outcomes=[[v] * len(h) for v, h in zip(credit["terminal"], histories)],
                shuffled=[credit["signals"][i] for i in (1, 2, 3, 0)],
                zero=[[0.0] * len(h) for h in histories])


def gradient_statistics(vectors):
    values = torch.tensor(vectors, dtype=torch.float64)
    if values.ndim != 2 or len(values) < 2 or not torch.isfinite(values).all():
        raise ValueError("Gradient statistics require finite independent batch vectors")
    mean = values.mean(dim=0)
    covariance_trace = float(((values - mean) ** 2).sum() / (len(values) - 1))
    return dict(batches=len(values), batch_norms=[float(g.norm()) for g in values],
                mean_norm=float(mean.norm()), sample_covariance_trace=covariance_trace,
                mean_norm_over_rms_dispersion=float(mean.norm()) / covariance_trace ** .5 if covariance_trace else None,
                pairwise_cosines=[[cosine(a, b) for b in values] for a in values],
                leave_one_out_cosines=[cosine(g, (values.sum(dim=0) - g) / (len(values) - 1)) for g in values])


def paired_quality(candidate, control):
    if not candidate or len(candidate) != len(control):
        raise ValueError("Prospective quality comparison requires matched episodes")
    results = []
    for a, b in zip(candidate, control):
        if a["seed"] != b["seed"]:
            raise ValueError("Prospective evaluation seed pairing mismatch")
        x, y = a["summary"], b["summary"]
        if x["root"] != y["root"] or x["work"]["proposals"] != y["work"]["proposals"]:
            raise ValueError("Prospective quality comparison changed root or effort")
        qa, qb = tuple(x["best_score"]["tuple"]), tuple(y["best_score"]["tuple"])
        results.append(dict(win=(qb > qa) - (qb < qa), delta=[u - v for u, v in zip(qa, qb)],
                            futile_delta=x["waste"]["futile_fraction"] - y["waste"]["futile_fraction"]))
    n = len(results)
    return dict(episodes=n, wins=sum(r["win"] > 0 for r in results), losses=sum(r["win"] < 0 for r in results),
                ties=sum(r["win"] == 0 for r in results), mean_win=sum(r["win"] for r in results) / n,
                mean_delta=[sum(r["delta"][i] for r in results) / n for i in range(3)],
                mean_futile_delta=sum(r["futile_delta"] for r in results) / n)


def bootstrap(values, count, bootstrap_seed):
    values = torch.tensor(values, dtype=torch.float64)
    if values.ndim != 1 or len(values) < 2 or not torch.isfinite(values).all() or count < 1:
        raise ValueError("Bootstrap requires finite independent batch means")
    indices = torch.randint(len(values), (count, len(values)), generator=torch.Generator().manual_seed(bootstrap_seed))
    means = values[indices].mean(dim=1)
    return dict(mean=float(values.mean()), percentile_95=[float(v) for v in torch.quantile(means, torch.tensor([.025, .975], dtype=torch.float64))],
                batches=len(values), resamples=count, unit="independent-update-batch")


def summarize(groups, count):
    roots = []
    for root_index in range(2):
        batches = [g for g in groups if g["root_index"] == root_index]
        if len(batches) != 8 or [g["batch_index"] for g in batches] != list(range(8)):
            raise ValueError("Incomplete predefined independent-batch allocation")
        arms = {}
        for arm_index, name in enumerate(ARMS):
            paired = [paired_quality(g["arms"][name]["evaluation"], g["arms"]["zero"]["evaluation"]) for g in batches]
            episodes = [e["summary"] for g in batches for e in g["arms"][name]["evaluation"]]
            arms[name] = dict(gradient=gradient_statistics([g["arms"][name]["gradient"] for g in batches]),
                paired_batches=paired, quality=bootstrap([p["mean_win"] for p in paired], count, seed("bootstrap", root_index, arm_index)),
                mean_component_delta=[sum(p["mean_delta"][i] for p in paired) / len(paired) for i in range(3)],
                mean_futile_fraction=sum(e["waste"]["futile_fraction"] for e in episodes) / len(episodes),
                route_expansions=sum(e["work"]["route_expansions"] for e in episodes),
                best_score=min(e["best_score"]["tuple"] for e in episodes),
                area_improvements=sum(e["best_score"]["area"] < e["initial_score"]["area"] for e in episodes))
        terminal = arms["terminal_same_outcomes"]["gradient"]["sample_covariance_trace"]
        roots.append(dict(root_index=root_index, root=batches[0]["root"], arms=arms,
                          prefix_to_terminal_covariance_trace=arms["prefix"]["gradient"]["sample_covariance_trace"] / terminal if terminal else None))
    return roots


def validate_config(config):
    fixed = dict(schema_version=1, seed_namespace="frozen-batch-v1", batches=8, workers=4, horizon=128,
                 evaluation_seeds=4, episode_ms=300000, output_bytes=2147483648, bootstrap_samples=2000)
    if any(type(config.get(k)) is not type(v) or config.get(k) != v for k, v in fixed.items()):
        raise ValueError("Configuration differs from the preregistered frozen-batch allocation")
    if len(config.get("capsules", [])) != 2 or not isinstance(config.get("plan"), str):
        raise ValueError("Expected two pinned source capsules and the real environment plan")


def run(config_path, output):
    config_path = Path(config_path).resolve()
    config = json.loads(config_path.read_bytes())
    validate_config(config)
    output = Path(output).resolve()
    output.mkdir()
    began = time.perf_counter()

    def guard():
        size = sum(p.stat().st_size for p in output.rglob("*") if p.is_file())
        if size > config["output_bytes"]:
            raise ValueError(f"Frozen-batch output exceeds {config['output_bytes']} bytes: {output}")

    def write(name, value):
        result = save_capsule(output / name, value)
        guard()
        return result

    references = [dict(ref, path=str((config_path.parent / ref["path"]).resolve())) for ref in config["capsules"]]
    capsules = []
    for ref in references:
        if Path(ref["path"]).stat().st_size > CAPSULE_BYTES:
            raise ValueError("Source capsule exceeds evidence byte guard")
        capsules.append(referenced_json(ref))
    configure_runtime(capsules[0]["before"]["runtime"])
    for capsule in capsules:
        verify(capsule)
    before = capsules[0]["before"]
    model, optimizer = restore_snapshot(before)
    if optimizer.state:
        raise ValueError("Frozen comparison requires the original empty Adam state")
    episode = referenced_json(capsules[0]["episodes"][0])
    if episode["best_score"]["tuple"] != [567, 86, 26]:
        raise ValueError("The predefined already-compacted diagnostic root changed")
    small = copy.deepcopy(capsules[0]["root"])
    small.update(id=small["id"] + "-retained-frozen-v1", generation="explicit-already-compacted-diagnostic-descendant",
                 witness=episode["best_witness"])
    roots = [small, capsules[1]["root"]]
    root_refs = [write(f"root-{i}.json", r) for i, r in enumerate(roots)]
    source_paths = {ref["path"] for c in capsules for ref in c["sources"]}
    source_paths.update(str(Path(__file__).with_name(n).resolve()) for n in
                        ("frozen_batch.py", "frozen-batch-contract.md", "gradient_audit.py"))
    source_paths.add(str(config_path))
    sources = [file_reference(p) for p in sorted(source_paths)]
    manifest = dict(schema_version=1, config=config, capsules=references, before_sha256=fingerprint(before),
                    roots=root_refs, descendant_source=capsules[0]["episodes"][0], sources=sources,
                    parameter_layout=[dict(name=k, shape=list(p.shape)) for k, p in model.named_parameters()])
    write("manifest.json", manifest)
    groups, group_refs, behavior_actions, evaluation_actions = [], [], 0, 0
    with Pool((config_path.parent / config["plan"]).resolve(), output, config["workers"]) as pool:
        for i, root in enumerate(roots):
            pool.workers[0].call("reset", root=root, budget=dict(steps=config["horizon"], elapsed_ms=config["episode_ms"]))
            write(f"root-audit-{i}.json", pool.workers[0].call("audit"))
        for root_index, root in enumerate(roots):
            for batch_index in range(config["batches"]):
                label = f"r{root_index}-b{batch_index}"
                model, _ = restore_snapshot(before)
                data = rollout(pool, model, [root] * config["workers"], config["horizon"], config,
                               seed("behavior", root_index, batch_index), record=True)
                histories = data["histories"]
                behavior_actions += sum(len(h) for h in histories)
                credit = credit_signals(histories, data["summaries"], "quality-futility-prefix")
                signals = signals_for(credit, histories)
                arms = {}
                for name in ARMS:
                    candidate, candidate_optimizer = restore_snapshot(before)
                    gradient, digest = raw_gradient(candidate, histories, signals[name], 0.0)
                    initial = vector(candidate)
                    update = apply_update(candidate, candidate_optimizer, histories, signals[name], 0.0)
                    if digest != update["gradient_sha256"]:
                        raise ValueError(f"Gradient fingerprint differs from real update: {label}/{name}")
                    if name == "zero" and not torch.equal(initial, vector(candidate)):
                        raise ValueError("Zero arm changed frozen-policy parameters")
                    after = snapshot(candidate, candidate_optimizer)
                    arms[name] = dict(signals=signals[name], gradient=gradient.tolist(), update=update, after=after, evaluation=[])
                    if name == "prefix":
                        capsule = dict(schema_version=1, scope="single-on-policy-adam-update", sources=sources,
                            root=root, root_artifact=root_refs[root_index], audits=data["audits"], episodes=data["artifacts"],
                            rollouts=pack_histories(histories), summaries=data["summaries"], credit_mode="quality-futility-prefix",
                            entropy_weight=0.0, credit=credit, before=before, after=after, update=update,
                            distribution_changes=distribution_changes(candidate, histories))
                        replay = verify(capsule)
                        capsule_ref = write(f"capsule-{label}.json", capsule)
                        del capsule
                    for index in range(config["evaluation_seeds"]):
                        evaluation_seed = seed("evaluation", root_index, batch_index, index)
                        evaluated = rollout(pool, candidate, [root], config["horizon"], config, evaluation_seed)
                        summary = evaluated["summaries"][0]
                        if summary["work"]["proposals"] != config["horizon"]:
                            raise ValueError(f"Prospective evaluation was truncated: {label}/{name}/{index}")
                        evaluation_actions += summary["work"]["proposals"]
                        arms[name]["evaluation"].append(dict(seed=evaluation_seed, summary=summary,
                            audit=evaluated["audits"][0], artifact=evaluated["artifacts"][0]))
                group = dict(schema_version=1, root_index=root_index, root=root["id"], batch_index=batch_index,
                             behavior_seed=seed("behavior", root_index, batch_index), behavior_capsule=capsule_ref,
                             prefix_replay=replay, arms=arms)
                groups.append(group)
                group_refs.append(write(f"batch-{label}.json", group))
                print(json.dumps(dict(stage="independent-batch-complete", root_index=root_index, batch_index=batch_index,
                    behavior_actions=behavior_actions, evaluation_actions=evaluation_actions,
                    paired={name: paired_quality(arms[name]["evaluation"], arms["zero"]["evaluation"]) for name in ARMS})), flush=True)
    for ref in sources:
        if file_reference(ref["path"]) != ref:
            raise ValueError(f"Frozen-batch source changed during execution: {ref['path']}")
    if behavior_actions != 8192 or evaluation_actions != 32768:
        raise ValueError("Completed action ledger differs from the preregistered allocation")
    result = dict(schema_version=1, status="complete", scope="two-fixed-root-single-update-response-pilot",
                  groups=group_refs, results=summarize(groups, config["bootstrap_samples"]),
                  behavior_actions=behavior_actions, evaluation_actions=evaluation_actions,
                  final_episode_audits=320, elapsed_seconds=time.perf_counter() - began, production_ready=False)
    reference = write("report.json", result)
    print(json.dumps(dict(status="complete", report=reference, elapsed_seconds=result["elapsed_seconds"])), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True)
    parser.add_argument("--output-directory", required=True)
    options = parser.parse_args()
    try:
        run(options.config, options.output_directory)
    except Exception as error:
        print(json.dumps(dict(code="frozen-batch-failed", severity="error", stage="frozen-batch-pilot",
                              message=str(error), entity=options.config)), flush=True)
        raise SystemExit(1)
