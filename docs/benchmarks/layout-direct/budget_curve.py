"""Cumulative streaming RL with fixed checkpoints and prospective matched controls."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import time

import torch

from client import Pool
from frozen_batch import paired_quality
from policy import DirectPolicy
from replay import (file_reference, referenced_json, snapshot, pack_histories,
                    save_capsule, verify)
from train import credit_signals, rollout
from update import apply_update, distribution_changes, fingerprint


def validate_config(config):
    fixed = dict(schema_version=1, seeds=[1901, 2901, 3901], checkpoints=[64, 256, 1024],
                 workers=4, horizon=512, evaluation_horizon=1024, evaluation_roots=12,
                 evaluation_seeds=[101, 211], generation_attempts_per_root=200,
                 minimum_training_facilities=2, target_interval=8, credit="quality-futility-prefix",
                 threads=4, width=32, learning_rate=.003, entropy=.0001, episode_ms=300000,
                 output_bytes=17179869184, minimum_free_bytes=21474836480, wall_seconds=86400)
    if any(type(config.get(k)) is not type(v) or config[k] != v for k, v in fixed.items()):
        raise ValueError("Configuration differs from the declared cumulative budget allocation")
    if not isinstance(config.get("plan"), str) or not config["plan"]:
        raise ValueError("Expected an explicit environment plan path")


def seed_for(role, model_seed, index):
    return int.from_bytes(hashlib.sha256(json.dumps(["budget-curve-v1", role, model_seed, index]).encode()).digest()[:8], "little")


def check_sources(sources):
    for source in sources:
        if file_reference(source["path"])["sha256"] != source["sha256"]:
            raise ValueError(f"Source changed during budget study: {source['path']}")


class Evidence:
    def __init__(self, config_path, output):
        self.config_path = Path(config_path).resolve()
        self.config = json.loads(self.config_path.read_bytes())
        validate_config(self.config)
        self.plan = (self.config_path.parent / self.config["plan"]).resolve()
        self.output = Path(output).resolve()
        self.output.mkdir()
        self.began = time.perf_counter()
        paths = [self.config_path, self.plan] + [Path(__file__).with_name(name) for name in
            ("budget_curve.py", "budget-curve-contract.md", "train.py", "policy.py", "update.py", "replay.py",
             "client.py", "frozen_batch.py", "gradient_audit.py")]
        self.sources = [file_reference(p) for p in paths]
        self.write("configuration.json", self.config)
        self.guard()

    def attach(self, pool):
        if not pool.metadata.get("dynamic_generation") or pool.metadata.get("policy_type") != "conditional-direct-parameters":
            raise ValueError("Expected the complete-factory direct-action stream")
        for worker in pool.workers:
            # Pool construction only waits for worker zero; handshake every worker before
            # reading files produced by its asynchronous startup.
            metadata = worker.call("hello")
            if metadata != pool.metadata:
                raise ValueError("Environment workers disagree on their runtime contract")
            for name in ("implementation.json", "generation/provenance.json"):
                self.sources.extend(json.loads((worker.output / name).read_bytes()))
        unique = {}
        for source in self.sources:
            if source["path"] in unique and unique[source["path"]] != source["sha256"]:
                raise ValueError(f"Conflicting source hashes: {source['path']}")
            unique[source["path"]] = source["sha256"]
        self.sources = [dict(path=p, sha256=h) for p, h in sorted(unique.items())]
        self.write("provenance.json", self.sources)

    def write(self, name, value):
        return save_capsule(self.output / name, value)

    def guard(self):
        size = sum(p.stat().st_size for p in self.output.rglob("*") if p.is_file())
        if size > self.config["output_bytes"]:
            raise ValueError(f"Evidence output exceeds declared byte guard: {self.output}")
        if shutil.disk_usage(self.output).free < self.config["minimum_free_bytes"]:
            raise ValueError(f"Free disk is below the declared reserve: {self.output}")
        if time.perf_counter() - self.began > self.config["wall_seconds"]:
            raise ValueError(f"Command exceeds declared wall-time guard: {self.output}")


class Stream:
    def __init__(self, worker, evidence, split, namespace, excluded_families=()):
        self.worker, self.evidence, self.split, self.namespace = worker, evidence, split, namespace
        self.ordinal, self.attempts = 0, []
        self.excluded = set(excluded_families)

    def generate(self):
        config = self.evidence.config
        for _ in range(config["generation_attempts_per_root"]):
            self.evidence.guard()
            result = self.worker.call("generate", seed=self.namespace, ordinal=self.ordinal, split=self.split, stratum="any")
            self.ordinal += 1
            root, row = result["root"], result["row"]
            eligible = root is not None and row["learning_support"]["supported"] and len(root["witness"]["placements"]) >= config["minimum_training_facilities"]
            if root is not None and root["family"] in self.excluded:
                raise ValueError(f"Training target family overlaps frozen validation: {root['family']}")
            row = dict(row, learning_eligible=eligible)
            self.attempts.append(row)
            self.evidence.write(f"generation-{self.split}-{self.ordinal:07d}.json", row)
            if eligible:
                print(json.dumps(dict(stage="budget-fresh-factory", split=self.split, ordinal=row["ordinal"],
                    root=root["id"], family=root["family"], facilities=len(root["witness"]["placements"]), stratum=row["size_stratum"])), flush=True)
                return root
        raise ValueError(f"Fresh {self.split}/any generation exhausted {config['generation_attempts_per_root']} attempts; no fallback")


def require_complete(result, roots, horizon):
    if any(len(result[key]) != len(roots) for key in ("summaries", "audits", "artifacts")):
        raise ValueError("Episode/audit allocation mismatch")
    for root, summary, audit in zip(roots, result["summaries"], result["audits"]):
        if summary["root"] != root["id"] or summary["work"]["proposals"] != horizon:
            raise ValueError(f"Truncated or wrong-root episode: {root['id']}")
        if not audit["coverage"]["factory_validated"] or not audit["validation"]["covered_geometry_validated"]:
            raise ValueError(f"Full-factory audit failed: {root['id']}")


def validate_evaluation(episodes, roots, config):
    expected = [(s, r["id"]) for s in config["evaluation_seeds"] for r in roots]
    if [(e["seed"], e["summary"]["root"]) for e in episodes] != expected:
        raise ValueError("Evaluation root/seed order or cardinality changed")
    require_complete(dict(summaries=[e["summary"] for e in episodes], audits=[e["audit"] for e in episodes],
                          artifacts=[e["artifact"] for e in episodes]),
                     roots * len(config["evaluation_seeds"]), config["evaluation_horizon"])


def audit_episode(artifact, summary):
    episode = referenced_json(artifact)
    keys = ("root", "initial_score", "best_score", "waste", "work")
    if any(episode[k] != summary[k] for k in keys) or len(episode["trace"]) != summary["work"]["proposals"]:
        raise ValueError(f"Persisted episode differs from summary: {artifact['path']}")


def prepare(options):
    evidence = Evidence(options.config, options.output_directory)
    config = evidence.config
    with Pool(evidence.plan, evidence.output, 1) as pool:
        evidence.attach(pool)
        stream = Stream(pool.workers[0], evidence, "validation", "budget-curve-validation-v1")
        roots = [stream.generate() for _ in range(config["evaluation_roots"])]
        roots.append(pool.workers[0].call("target"))
        audits = []
        for root in roots:
            pool.workers[0].call("reset", root=root, budget=dict(steps=1, elapsed_ms=config["episode_ms"]))
            audits.append(pool.workers[0].call("audit"))
        if any(not a["coverage"]["factory_validated"] or not a["validation"]["covered_geometry_validated"] for a in audits):
            raise ValueError("Frozen evaluation root audit failed")
        value = dict(schema_version=1, scope="cumulative-budget-evaluation-roots", roots=roots, audits=audits,
                     root_fingerprints=[fingerprint(r) for r in roots], sources=evidence.sources, generation=stream.attempts)
    check_sources(evidence.sources)
    evidence.guard()
    reference = evidence.write("evaluation-set.json", value)
    print(json.dumps(dict(stage="budget-evaluation-set-frozen", reference=reference, roots=len(roots))), flush=True)


def load_evaluation(reference, config):
    value = referenced_json(reference)
    if value.get("schema_version") != 1 or value.get("scope") != "cumulative-budget-evaluation-roots":
        raise ValueError("Unsupported frozen evaluation set")
    roots = value["roots"]
    if len(roots) != config["evaluation_roots"] + 1 or len({r["id"] for r in roots}) != len(roots):
        raise ValueError("Frozen root allocation mismatch")
    if ([fingerprint(r) for r in roots] != value["root_fingerprints"] or
        any(r["split"] != "validation" for r in roots[:-1]) or roots[-1]["generation"] != "explicit-target-replay"):
        raise ValueError("Frozen evaluation roots or split changed")
    if len(value["audits"]) != len(roots) or any(not a["coverage"]["factory_validated"] or not a["validation"]["covered_geometry_validated"] for a in value["audits"]):
        raise ValueError("Frozen set lacks complete successful audits")
    check_sources(value["sources"])
    return value


def evaluate(pool, model, optimizer, roots, evidence, step, arm, *, uniform=False):
    before = fingerprint(snapshot(model, optimizer))
    config, episodes = evidence.config, []
    with torch.random.fork_rng(devices=[]):
        for sampling_seed in config["evaluation_seeds"]:
            for offset in range(0, len(roots), config["workers"]):
                batch = roots[offset:offset + config["workers"]]
                result = rollout(pool, model, batch, config["evaluation_horizon"], config,
                                 seed_for("evaluation", 0, sampling_seed), uniform=uniform)
                require_complete(result, batch, config["evaluation_horizon"])
                episodes.extend(dict(seed=sampling_seed, summary=s, audit=a, artifact=p, inference_seconds=t)
                    for s, a, p, t in zip(result["summaries"], result["audits"], result["artifacts"], result["inference_seconds_by_worker"]))
                evidence.guard()
    if before != fingerprint(snapshot(model, optimizer)):
        raise ValueError("Evaluation changed the training model, Adam state or RNG")
    validate_evaluation(episodes, roots, config)
    value = dict(checkpoint=step, arm=arm, episodes=episodes, training_state_sha256=before)
    evidence.write(f"evaluation-{step:04d}-{arm}.json", value)
    print(json.dumps(dict(stage="budget-checkpoint-evaluated", checkpoint=step, arm=arm,
                         episodes=len(episodes), target=[e["summary"]["best_score"]["tuple"] for e in episodes if e["summary"]["root"] == roots[-1]["id"]])), flush=True)
    return value


def partitions(episodes, target_id):
    return {name: [e for e in episodes if (e["summary"]["root"] == target_id) == target]
            for name, target in (("development", False), ("target", True))}


def compare(candidate, control, target_id):
    a, b = partitions(candidate, target_id), partitions(control, target_id)
    return {name: paired_quality(a[name], b[name]) for name in a}


def train(options):
    evidence = Evidence(options.config, options.output_directory)
    config = evidence.config
    if options.seed not in config["seeds"]:
        raise ValueError("Seed is not in the declared independent training allocation")
    evaluation_reference = dict(path=str(Path(options.evaluation_set).resolve()), sha256=options.evaluation_sha256)
    frozen = load_evaluation(evaluation_reference, config)
    evidence.write("evaluation-reference.json", evaluation_reference)
    roots = frozen["roots"]
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.manual_seed(options.seed)
    model = DirectPolicy(config["width"])
    optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
    state = snapshot(model, optimizer)
    evidence.write("checkpoint-0000.json", state)
    previous = fingerprint(state)
    groups, evaluations = [], []
    with Pool(evidence.plan, evidence.output, config["workers"]) as pool:
        evidence.attach(pool)
        if fingerprint(pool.workers[0].call("target")) != fingerprint(roots[-1]):
            raise ValueError("Training target differs from frozen full target")
        stream = Stream(pool.workers[0], evidence, "train", f"budget-curve-train-v1-{options.seed}", [r["family"] for r in roots[:-1]])
        for arm in ("initial", "uniform"):
            evaluations.append(evaluate(pool, model, optimizer, roots, evidence, 0, arm, uniform=arm == "uniform"))
        for group in range(1, config["checkpoints"][-1] + 1):
            evidence.guard()
            began = time.perf_counter()
            prior = snapshot(model, optimizer)
            if fingerprint(prior) != previous:
                raise ValueError(f"Cumulative optimizer continuity broken before group {group}")
            root = roots[-1] if group % config["target_interval"] == 0 else stream.generate()
            root_artifact = evidence.write(f"root-{group:04d}.json", root)
            rollout_seed = seed_for("training", options.seed, group)
            result = rollout(pool, model, [root] * config["workers"], config["horizon"], config, rollout_seed, record=True)
            require_complete(result, [root] * config["workers"], config["horizon"])
            credit = credit_signals(result["histories"], result["summaries"], config["credit"])
            start = time.perf_counter()
            update = apply_update(model, optimizer, result["histories"], credit["signals"], config["entropy"])
            update_seconds = time.perf_counter() - start
            state = snapshot(model, optimizer)
            after = fingerprint(state)
            capsule, replay = None, None
            if group == 1 or group in config["checkpoints"]:
                changes = distribution_changes(model, result["histories"])
                capsule = evidence.write(f"update-{group:04d}.json", dict(schema_version=1, scope="single-on-policy-adam-update",
                    group=group, sources=evidence.sources, root=root, root_artifact=root_artifact, rollout_seed=rollout_seed,
                    episodes=result["artifacts"], audits=result["audits"], rollouts=pack_histories(result["histories"]),
                    summaries=result["summaries"], credit_mode=config["credit"], entropy_weight=config["entropy"],
                    credit=credit, before=prior, after=state, update=update, distribution_changes=changes))
                with torch.random.fork_rng(devices=[]):
                    replay = verify(referenced_json(capsule))
                if fingerprint(snapshot(model, optimizer)) != after:
                    raise ValueError("Offline replay changed the live learner")
            row = dict(group=group, seed=options.seed, root=root_artifact, family=root["family"], generation=root["generation"],
                facilities=len(root["witness"]["placements"]), sampled_transitions=sum(map(len, result["histories"])),
                before_sha256=previous, after_sha256=after, rollout_seed=rollout_seed, update=update, update_seconds=update_seconds,
                terminal_outcomes=credit["outcomes"], terminal_advantages=credit["terminal"], quality_terminal_advantages=credit["quality_terminal"],
                credited_transitions=sum(v != 0 for xs in credit["signals"] for v in xs),
                summaries=result["summaries"], audits=result["audits"], artifacts=result["artifacts"],
                inference_seconds=result["inference_seconds"], capsule=capsule, replay=replay, elapsed_seconds=time.perf_counter() - began)
            evidence.write(f"group-{group:04d}.json", row)
            groups.append(row)
            previous = after
            print(json.dumps(dict(stage="budget-cumulative-update", seed=options.seed, group=group, facilities=row["facilities"],
                informative=any(credit["quality_terminal"]), credited=row["credited_transitions"], gradient_norm=update["gradient_norm"],
                best=[s["best_score"]["tuple"] for s in result["summaries"]], elapsed_seconds=row["elapsed_seconds"])), flush=True)
            # Do not retain the final group's large trajectory while collecting fresh evaluation.
            del result, prior, credit
            if group in config["checkpoints"]:
                evidence.write(f"checkpoint-{group:04d}.json", state)
                evaluation = evaluate(pool, model, optimizer, roots, evidence, group, "trained")
                evaluations.append(evaluation)
                comparison = compare(evaluation["episodes"], evaluations[0]["episodes"], roots[-1]["id"])
                evidence.write(f"comparison-{group:04d}.json", comparison)
                print(json.dumps(dict(stage="budget-quality", seed=options.seed, checkpoint=group, against_initial=comparison)), flush=True)
                check_sources(evidence.sources + frozen["sources"])
    check_sources(evidence.sources + frozen["sources"])
    evidence.guard()
    result = dict(schema_version=1, status="complete", scope="cumulative-streaming-budget-curve", seed=options.seed,
        config=config, sources=evidence.sources, evaluation_set=evaluation_reference, groups=groups, evaluations=evaluations,
        generation=stream.attempts, elapsed_seconds=time.perf_counter() - evidence.began)
    reference = evidence.write("report.json", result)
    print(json.dumps(dict(stage="budget-seed-complete", seed=options.seed, reference=reference)), flush=True)


def analyze(options):
    config = json.loads(Path(options.config).read_bytes())
    validate_config(config)
    reports = [json.loads(Path(p).read_bytes()) for p in options.reports]
    if sorted(r["seed"] for r in reports) != sorted(config["seeds"]):
        raise ValueError("Analysis requires every declared independent seed exactly once")
    reports.sort(key=lambda r: config["seeds"].index(r["seed"]))
    references = [r["evaluation_set"] for r in reports]
    if any(r != references[0] for r in references):
        raise ValueError("Independent seeds did not use the identical frozen evaluation set")
    frozen = load_evaluation(references[0], config)
    target = frozen["roots"][-1]["id"]
    curves = []
    directories = {json.loads(Path(p).read_bytes())["seed"]: Path(p).resolve().parent for p in options.reports}
    for report in reports:
        if report.get("status") != "complete" or report.get("scope") != "cumulative-streaming-budget-curve" or report["config"] != config:
            raise ValueError("Incomplete or changed-config budget run")
        check_sources(report["sources"])
        directory = directories[report["seed"]]
        groups = report["groups"]
        if [g["group"] for g in groups] != list(range(1, config["checkpoints"][-1] + 1)):
            raise ValueError("Incomplete cumulative update allocation")
        for i, group in enumerate(groups):
            if json.loads((directory / f"group-{group['group']:04d}.json").read_bytes()) != group:
                raise ValueError("Aggregate group differs from persisted group")
            if i and group["before_sha256"] != groups[i - 1]["after_sha256"]:
                raise ValueError("Persisted optimizer continuity mismatch")
            if group["sampled_transitions"] != config["workers"] * config["horizon"]:
                raise ValueError("Persisted training effort mismatch")
            root = referenced_json(group["root"])
            require_complete(group, [root] * config["workers"], config["horizon"])
            for artifact, summary in zip(group["artifacts"], group["summaries"]):
                audit_episode(artifact, summary)
        if fingerprint(json.loads((directory / "checkpoint-0000.json").read_bytes())) != groups[0]["before_sha256"]:
            raise ValueError("Initial snapshot does not begin the persisted state chain")
        evaluations = report["evaluations"]
        if [(e["checkpoint"], e["arm"]) for e in evaluations] != [(0, "initial"), (0, "uniform")] + [(c, "trained") for c in config["checkpoints"]]:
            raise ValueError("Incomplete fixed-checkpoint evaluation allocation")
        for evaluation in evaluations:
            validate_evaluation(evaluation["episodes"], frozen["roots"], config)
            step = evaluation["checkpoint"]
            state_hash = fingerprint(json.loads((directory / f"checkpoint-{step:04d}.json").read_bytes()))
            expected_hash = groups[step - 1]["after_sha256"] if step else groups[0]["before_sha256"]
            if state_hash != expected_hash or state_hash != evaluation["training_state_sha256"]:
                raise ValueError("Evaluated checkpoint differs from cumulative training chain")
            for episode in evaluation["episodes"]:
                audit_episode(episode["artifact"], episode["summary"])
        previous = evaluations[0]
        checkpoints = []
        for current in evaluations[2:]:
            checkpoints.append(dict(checkpoint=current["checkpoint"], against_initial=compare(current["episodes"], evaluations[0]["episodes"], target),
                against_uniform=compare(current["episodes"], evaluations[1]["episodes"], target),
                against_previous=compare(current["episodes"], previous["episodes"], target)))
            previous = current
        curves.append(dict(seed=report["seed"], checkpoints=checkpoints))
    summary = []
    for i, checkpoint in enumerate(config["checkpoints"]):
        endpoints = [c["checkpoints"][i]["against_initial"]["development"]["mean_win"] for c in curves]
        summary.append(dict(checkpoint=checkpoint, seed_endpoints=endpoints, mean=sum(endpoints) / len(endpoints),
                            minimum=min(endpoints), maximum=max(endpoints), independent_unit="training-seed"))
    output = dict(schema_version=1, scope="three-seed-finite-budget-response", status="complete", curves=curves, development_summary=summary,
                  sources=[file_reference(p) for p in options.reports], evaluation_set=references[0])
    reference = save_capsule(Path(options.output), output)
    print(json.dumps(dict(stage="budget-curve-analyzed", reference=reference, development_summary=summary)), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "train", "analyze"):
        command = sub.add_parser(name)
        command.add_argument("--config", required=True)
        if name == "analyze":
            command.add_argument("--reports", nargs=3, required=True)
            command.add_argument("--output", required=True)
        else:
            command.add_argument("--output-directory", required=True)
        if name == "train":
            command.add_argument("--seed", type=int, required=True)
            command.add_argument("--evaluation-set", required=True)
            command.add_argument("--evaluation-sha256", dest="evaluation_sha256", required=True)
    options = parser.parse_args()
    try:
        {"prepare": prepare, "train": train, "analyze": analyze}[options.command](options)
    except Exception as error:
        print(json.dumps(dict(code="budget-curve-failed", severity="error", stage="cumulative-budget-curve",
                              message=str(error), entity=getattr(options, "output_directory", getattr(options, "output", None)))), flush=True)
        raise SystemExit(1)
