"""Fresh factory groups, direct parameter rollouts, terminal-return updates and frozen controls."""

import argparse
import hashlib
import json
from pathlib import Path
import time

import torch

from client import Pool
from policy import DirectPolicy, decode, checkpoint, restore
from update import apply_update, behavior, distribution_changes
from replay import snapshot, pack_histories, save_capsule, file_reference


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, allow_nan=False)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate(config):
    if config.get("schema_version") != 3:
        raise ValueError("Unsupported streaming training schema")
    if config.get("credit") not in ("terminal-score", "quality-futility-prefix"):
        raise ValueError("Invalid streaming credit contract")
    for key in ("training_size_schedule", "evaluation_size_schedule"):
        if not isinstance(config.get(key), list) or not config[key] or any(s not in ("any", "small", "medium", "large") for s in config[key]):
            raise ValueError(f"Invalid required {key}")
    for key, limit in (("groups", 10000), ("workers", 16), ("horizon", 16384), ("threads", 16), ("width", 256),
                       ("target_interval", 10000), ("generation_attempts_per_root", 1000), ("episode_ms", 300000),
                       ("evaluation_roots", 300), ("minimum_training_facilities", 128)):
        if type(config.get(key)) is not int or not 1 <= config[key] <= limit:
            raise ValueError(f"Invalid {key}")
    if config["workers"] < 2 or config["width"] % 2 or type(config.get("seed")) is not int:
        raise ValueError("Expected multiple trajectories, even width and integer seed")
    if not 0 < config.get("learning_rate", 0) <= .1 or not 0 <= config.get("entropy", -1) <= .1:
        raise ValueError("Invalid optimizer configuration")
    if not config.get("evaluation_horizons") or any(type(n) is not int or not 1 <= n <= 16384 for n in config["evaluation_horizons"]):
        raise ValueError("Invalid evaluation horizons")
    if not config.get("evaluation_seeds") or any(type(n) is not int for n in config["evaluation_seeds"]):
        raise ValueError("Invalid evaluation seeds")


def terminal_advantages(values):
    scores = [tuple(v) for v in values]
    if len(scores) < 2:
        raise ValueError("Terminal comparison requires at least two trajectories")
    return [sum((b > a) - (b < a) for b in scores) / (len(scores) - 1) for a in scores]


def credit_signals(histories, summaries, mode):
    if len({len(h) for h in histories}) != 1 or not histories or not histories[0]:
        raise ValueError("Temporal credit requires nonempty equal-length trajectories")
    quality = [s["best_score"]["tuple"] for s in summaries]
    outcomes = [q + [s["waste"]["futile"]] if mode == "quality-futility-prefix" else q for q, s in zip(quality, summaries)]
    terminal = terminal_advantages(outcomes)
    signals = [[v] * len(h) for v, h in zip(terminal, histories)]
    if mode == "quality-futility-prefix":
        for t in range(len(histories[0])):
            prefix = terminal_advantages([h[t][0]["best_score"] + [h[t][0]["futile"]] for h in histories])
            for i in range(len(histories)):
                signals[i][t] -= prefix[i]
    elif mode != "terminal-score":
        raise ValueError("Unknown temporal credit mode")
    return dict(outcomes=outcomes, terminal=terminal, quality_terminal=terminal_advantages(quality), signals=signals)


def rng(seed, root, worker=0):
    value = int.from_bytes(hashlib.sha256(json.dumps([seed, root, worker]).encode()).digest()[:8], "little")
    return torch.Generator().manual_seed(value)


def rollout(pool, model, roots, horizon, config, seed, *, uniform=False, record=False):
    workers = pool.workers[:len(roots)]
    obs = [f.result() for f in [pool.executor.submit(w.call, "reset", root=r, budget=dict(steps=horizon, elapsed_ms=config["episode_ms"])) for w, r in zip(workers, roots)]]
    generators = [rng(seed, r["id"], i if record else 0) for i, r in enumerate(roots)]
    histories = [[] for _ in workers]
    states = [{} for _ in workers]
    inference = [0.0 for _ in workers]
    trace_timing = {}
    behavior_capture_seconds = 0.0
    futile = [0 for _ in workers]
    seen = [{o["state"]["state"]} for o in obs]
    for step in range(horizon):
        active = [i for i, o in enumerate(obs) if not o["done"]]
        if not active:
            break
        actions, pending = {}, {}
        with torch.no_grad():
            for i in active:
                start = time.perf_counter()
                trace = [] if record else None
                action, decisions, logp, entropy = decode(model, obs[i], generator=generators[i], uniform=uniform, trace=trace, timing=trace_timing)
                inference[i] += time.perf_counter() - start
                actions[i] = action
                if record:
                    capture_start = time.perf_counter()
                    identity = obs[i]["state"]["state"]
                    state = states[i].setdefault(identity, obs[i]["state"])
                    pending[i] = (dict(obs[i], state=state, futile=futile[i], behavior=behavior(logp, entropy, trace)), decisions, action)
                    behavior_capture_seconds += time.perf_counter() - capture_start
        futures = [(i, pool.executor.submit(workers[i].call, "step", action=actions[i])) for i in active]
        for i, future in futures:
            obs[i] = future.result()
            transition = obs[i]["transition"]
            if transition.get("performed") is False:
                continue
            if record:
                histories[i].append(pending[i])
            if not transition["accepted"] or transition["witness_sha256"] in seen[i]:
                futile[i] += 1
            if transition["accepted"]:
                seen[i].add(transition["witness_sha256"])
    summaries = [w.call("summary") for w in workers]
    if any(s["waste"]["futile"] != n for s, n in zip(summaries, futile)):
        raise RuntimeError("Python and worker futile-action accounting disagree")
    if record and any(len(h) != horizon for h in histories):
        raise RuntimeError("Training horizon was truncated; do not rank unequal effort as equal trajectories")
    audits = [w.call("audit") for w in workers]
    artifacts = [w.call("save") for w in workers]
    return dict(observations=obs, histories=histories, summaries=summaries, audits=audits, artifacts=artifacts,
                recording=dict(decoder_trace_seconds=trace_timing.get("trace_seconds", 0.0), behavior_capture_seconds=behavior_capture_seconds),
                inference_seconds=sum(inference), inference_seconds_by_worker=inference)


def run(options):
    config = read(options.config)
    if options.seed is not None:
        config["seed"] = options.seed
    validate(config)
    output = Path(options.output_directory).resolve()
    output.mkdir()
    provenance = [dict(path=str(p.resolve()), sha256=sha(p)) for p in
                  [Path(__file__), *[Path(__file__).with_name(name) for name in ("client.py", "policy.py", "update.py", "replay.py")],
                   Path(options.plan), Path(options.config)]]
    write(output / "provenance.json", provenance)
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.manual_seed(config["seed"])
    began = time.perf_counter()
    generation, groups, evaluation = [], [], []
    ordinals = dict(train=0, validation=0)
    with Pool(Path(options.plan).resolve(), output, config["workers"]) as pool:
        if not pool.metadata.get("dynamic_generation") or pool.metadata.get("policy_type") != "conditional-direct-parameters":
            raise ValueError("Expected a dynamic direct-action worker")
        environment_sources = read(pool.workers[0].output / "implementation.json") + read(pool.workers[0].output / "generation/provenance.json")

        def generate(split, stratum):
            # The validation stream is fixed across model seeds and never used for training.
            seed = f"stream-train-main-v1-{config['seed']}" if split == "train" else "stream-validation-frozen-v1"
            for _ in range(config["generation_attempts_per_root"]):
                result = pool.workers[0].call("generate", seed=seed, ordinal=ordinals[split], split=split, stratum=stratum)
                ordinals[split] += 1
                root, row = result["root"], result["row"]
                supported = root is not None and row["learning_support"]["supported"]
                large_enough = root is not None and len(root["witness"]["placements"]) >= config["minimum_training_facilities"]
                eligible = supported and large_enough
                eligibility = "requested-stratum-filtered" if row["status"] == "size-stratum-filtered" else "eligible" if eligible else "construction-unavailable" if root is None else "observation-guard" if not supported else "below-training-minimum"
                generation.append(dict(**row, learning_eligible=eligible, learning_eligibility=eligibility))
                if not eligible and (row.get("facilities", 0) >= 17 or ordinals[split] % 10 == 0):
                    print(json.dumps(dict(stage="generation-attempt", split=split, ordinal=row["ordinal"], status=row["status"],
                                          facilities=row.get("facilities"), stratum=row["size_stratum"], eligibility=eligibility,
                                          topology=row.get("internal_topology"), elapsed_ms=row["elapsed_ms"])), flush=True)
                if eligible:
                    print(json.dumps(dict(stage="fresh-factory", split=split, ordinal=row["ordinal"], target=row["sample"]["request"]["target"]["item"],
                                          facilities=row["facilities"], stratum=row["size_stratum"], load=row["sample"]["target_load"],
                                          transports=row["transports"], internal_topology=row["internal_topology"])), flush=True)
                    return root
            raise RuntimeError(f"Fresh {split}/{stratum} factory generation exhausted {config['generation_attempts_per_root']} attempts; no easier-stratum or stored-root fallback")

        target = pool.workers[0].call("target")
        model = DirectPolicy(config["width"])
        initial = checkpoint(model)
        before = {k: v.detach().clone() for k, v in model.state_dict().items()}
        write(output / "initial-model.json", initial)
        optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
        print(json.dumps(dict(stage="streaming-training-started", groups=config["groups"], horizon=config["horizon"],
                              parameters=sum(p.numel() for p in model.parameters()))), flush=True)
        fresh_group = 0
        for group in range(config["groups"]):
            started = time.perf_counter()
            if (group + 1) % config["target_interval"] == 0:
                root = target
            else:
                schedule = config["training_size_schedule"]
                root = generate("train", schedule[fresh_group % len(schedule)])
                fresh_group += 1
            result = rollout(pool, model, [root] * config["workers"], config["horizon"], config, config["seed"] + group, record=True)
            credit = credit_signals(result["histories"], result["summaries"], config["credit"])
            advantage = credit["terminal"]
            samples = sum(len(h) for h in result["histories"])
            if not samples:
                raise RuntimeError("No training action fit the declared episode budget")
            evidence_start = time.perf_counter()
            root_path = output / f"root-{group + 1:04d}.json"
            write(root_path, root)
            root_artifact = file_reference(root_path)
            prior = snapshot(model, optimizer)
            packed = pack_histories(result["histories"])
            evidence_seconds = time.perf_counter() - evidence_start
            update_start = time.perf_counter()
            update_timing = {}
            update = apply_update(model, optimizer, result["histories"], credit["signals"], config["entropy"], timing=update_timing)
            update_seconds = time.perf_counter() - update_start
            evidence_start = time.perf_counter()
            after = snapshot(model, optimizer)
            changes = distribution_changes(model, result["histories"])
            capsule = save_capsule(output / f"update-{group + 1:04d}.json", dict(
                schema_version=1, scope="single-on-policy-adam-update", group=group + 1, sources=provenance + environment_sources,
                root=root, root_artifact=root_artifact, rollout_seed=config["seed"] + group, episodes=result["artifacts"], audits=result["audits"],
                rollouts=packed, summaries=result["summaries"], credit_mode=config["credit"], entropy_weight=config["entropy"],
                credit=credit, before=prior, after=after, update=update, distribution_changes=changes))
            evidence_seconds += time.perf_counter() - evidence_start
            row = dict(group=group + 1, root=root["id"], family=root["family"], generation=root["generation"], facilities=len(root["witness"]["placements"]),
                       sampled_transitions=samples, terminal_advantages=advantage, **update,
                       quality_terminal_advantages=credit["quality_terminal"], terminal_outcomes=credit["outcomes"],
                       credited_transitions=sum(v != 0 for xs in credit["signals"] for v in xs),
                       inference_seconds=result["inference_seconds"], update_seconds=update_seconds,
                       update_capsule=capsule, post_rollout_evidence_seconds=evidence_seconds, distribution_changes=changes,
                       recording=dict(**result["recording"], update_recording_seconds=sum(update_timing.values()), update_components=update_timing,
                                      note="Decoder trace is inside inference; update recording is inside update; do not add these subsets twice."),
                       summaries=result["summaries"], audits=result["audits"], artifacts=result["artifacts"], elapsed_seconds=time.perf_counter() - started)
            groups.append(row)
            write(output / f"group-{group + 1:04d}.json", row)
            write(output / f"checkpoint-{group + 1:04d}.json", checkpoint(model))
            print(json.dumps(dict(stage="streaming-optimizer-update", group=group + 1, root=root["id"], informative=any(advantage),
                                  quality_informative=any(credit["quality_terminal"]), credited=row["credited_transitions"],
                                  futile=[s["waste"]["futile_fraction"] for s in result["summaries"]],
                                  best=[s["best_score"]["tuple"] for s in result["summaries"]], elapsed_seconds=row["elapsed_seconds"])), flush=True)
        final = checkpoint(model)
        restored = restore(final)
        reference_obs, decisions, _ = result["histories"][0][-1]
        with torch.no_grad():
            a = decode(model, reference_obs, decisions=decisions)
            b = decode(restored, reference_obs, decisions=decisions)
            if a[0] != b[0] or not torch.equal(a[2], b[2]):
                raise RuntimeError("Direct checkpoint round trip changed predictions")
        write(output / "model.json", final)
        training = dict(groups=groups, optimizer_steps=len(groups), informative_groups=sum(any(g["terminal_advantages"]) for g in groups),
                        quality_informative_groups=sum(any(g["quality_terminal_advantages"]) for g in groups),
                        credited_transitions=sum(g["credited_transitions"] for g in groups),
                        changed_weight_elements=sum(int(torch.count_nonzero(v.detach() - before[k])) for k, v in model.state_dict().items()),
                        sampled_transitions=sum(g["sampled_transitions"] for g in groups),
                        dynamic_groups=sum(g["generation"] == "dynamic-catalog" for g in groups),
                        unique_dynamic_roots=len({g["root"] for g in groups if g["generation"] == "dynamic-catalog"}))
        write(output / "training.json", training)
        schedule = config["evaluation_size_schedule"]
        roots = [generate("validation", schedule[i % len(schedule)]) for i in range(config["evaluation_roots"])] + [target]
        write(output / "evaluation-roots.json", roots)
        for horizon in config["evaluation_horizons"]:
            for seed in config["evaluation_seeds"]:
                for name, policy in [("initial", restore(initial)), ("uniform", None), ("trained", model)]:
                    for start in range(0, len(roots), config["workers"]):
                        batch = roots[start:start + config["workers"]]
                        result = rollout(pool, policy, batch, horizon, config, seed, uniform=name == "uniform")
                        for i, root in enumerate(batch):
                            row = dict(model=name, root=root["id"], family=root["family"], split=root["split"], horizon=horizon, seed=seed, facilities=len(root["witness"]["placements"]),
                                       inference_seconds=result["inference_seconds_by_worker"][i],
                                       inference_batch_seconds=result["inference_seconds"], batch_roots=[r["id"] for r in batch],
                                       summary=result["summaries"][i], audit=result["audits"][i], artifact=result["artifacts"][i])
                            evaluation.append(row)
                            write(output / f"evaluation-{len(evaluation):04d}.json", row)
                            print(json.dumps(dict(stage="streaming-evaluation", model=name, root=root["id"], horizon=horizon,
                                                  proposals=row["summary"]["work"]["proposals"], best=row["summary"]["best_score"]["tuple"])), flush=True)
    for p in provenance:
        if sha(p["path"]) != p["sha256"]:
            raise RuntimeError(f'Source changed during streaming training: {p["path"]}')
    write(output / "report.json", dict(schema_version=1, status="complete", scope="dynamic-factory-direct-action-training", config=config,
                                      training=training, evaluation=evaluation, generation=generation, provenance=provenance,
                                      torch=torch.__version__, test_evaluations=0, elapsed_seconds=time.perf_counter() - began))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--plan", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--output-directory", required=True)
    parser.add_argument("--seed", type=int, help="Explicit model/training-stream seed override, recorded in the report")
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="streaming-training-failed", severity="error", stage="training", message=str(error), entity=None)), flush=True)
        raise SystemExit(1)
