"""Fresh factory groups, direct parameter rollouts, terminal-return updates and frozen controls."""

import argparse
import hashlib
import json
from pathlib import Path
import time

import torch

from client import Pool
from policy import DirectPolicy, decode, checkpoint, restore


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate(config):
    if config.get("schema_version") != 1:
        raise ValueError("Unsupported streaming training schema")
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


def rng(seed, root, worker=0):
    value = int.from_bytes(hashlib.sha256(json.dumps([seed, root, worker]).encode()).digest()[:8], "little")
    return torch.Generator().manual_seed(value)


def rollout(pool, model, roots, horizon, config, seed, *, uniform=False, record=False):
    workers = pool.workers[:len(roots)]
    obs = [f.result() for f in [pool.executor.submit(w.call, "reset", root=r, budget=dict(steps=horizon, elapsed_ms=config["episode_ms"])) for w, r in zip(workers, roots)]]
    generators = [rng(seed, r["id"], i if record else 0) for i, r in enumerate(roots)]
    histories = [[] for _ in workers]
    states = [{} for _ in workers]
    inference = 0.0
    for step in range(horizon):
        active = [i for i, o in enumerate(obs) if not o["done"]]
        if not active:
            break
        start = time.perf_counter()
        actions = {}
        with torch.no_grad():
            for i in active:
                action, decisions, _, _ = decode(model, obs[i], generator=generators[i], uniform=uniform)
                actions[i] = action
                if record:
                    identity = obs[i]["state"]["state"]
                    state = states[i].setdefault(identity, obs[i]["state"])
                    histories[i].append((dict(obs[i], state=state), decisions, action))
        inference += time.perf_counter() - start
        futures = [(i, pool.executor.submit(workers[i].call, "step", action=actions[i])) for i in active]
        for i, future in futures:
            obs[i] = future.result()
    summaries = [w.call("summary") for w in workers]
    audits = [w.call("audit") for w in workers]
    artifacts = [w.call("save") for w in workers]
    return dict(observations=obs, histories=histories, summaries=summaries, audits=audits, artifacts=artifacts, inference_seconds=inference)


def run(options):
    config = read(options.config)
    validate(config)
    output = Path(options.output_directory).resolve()
    output.mkdir()
    provenance = [dict(path=str(p.resolve()), sha256=sha(p)) for p in
                  [Path(__file__), Path(__file__).with_name("client.py"), Path(__file__).with_name("policy.py"), Path(options.plan), Path(options.config)]]
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

        def generate(split):
            # The validation stream is fixed across model seeds and never used for training.
            seed = f"stream-train-main-v1-{config['seed']}" if split == "train" else "stream-validation-frozen-v1"
            for _ in range(config["generation_attempts_per_root"]):
                result = pool.workers[0].call("generate", seed=seed, ordinal=ordinals[split], split=split)
                ordinals[split] += 1
                root, row = result["root"], result["row"]
                eligible = root is not None and len(root["witness"]["placements"]) >= config["minimum_training_facilities"]
                generation.append(dict(**row, learning_eligible=eligible))
                if eligible:
                    print(json.dumps(dict(stage="fresh-factory", split=split, ordinal=row["ordinal"], target=row["sample"]["request"]["target"]["item"],
                                          facilities=row["facilities"], transports=row["transports"], source_cycles=row["sample"]["source_cycles"])), flush=True)
                    return root
            raise RuntimeError(f"Fresh {split} factory generation exhausted {config['generation_attempts_per_root']} attempts; no stored-root fallback")

        target = pool.workers[0].call("target")
        model = DirectPolicy(config["width"])
        initial = checkpoint(model)
        before = {k: v.detach().clone() for k, v in model.state_dict().items()}
        write(output / "initial-model.json", initial)
        optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
        print(json.dumps(dict(stage="streaming-training-started", groups=config["groups"], horizon=config["horizon"],
                              parameters=sum(p.numel() for p in model.parameters()))), flush=True)
        for group in range(config["groups"]):
            started = time.perf_counter()
            root = target if (group + 1) % config["target_interval"] == 0 else generate("train")
            result = rollout(pool, model, [root] * config["workers"], config["horizon"], config, config["seed"] + group, record=True)
            advantage = terminal_advantages([o["best_score"] for o in result["observations"]])
            samples = sum(len(h) for h in result["histories"])
            if not samples:
                raise RuntimeError("No training action fit the declared episode budget")
            optimizer.zero_grad(set_to_none=True)
            task_loss = 0.0
            entropy_sum = 0.0
            update_start = time.perf_counter()
            # Recompute on-policy decisions before the single update, bounding autograd memory.
            for signal, history in zip(advantage, result["histories"]):
                for obs, decisions, original_action in history:
                    action, _, logp, entropy = decode(model, obs, decisions=decisions)
                    if action != original_action:
                        raise RuntimeError("Direct action replay changed before optimizer update")
                    loss = (-signal * logp - config["entropy"] * entropy) / samples
                    loss.backward()
                    task_loss += float((-signal * logp).detach()) / samples
                    entropy_sum += float(entropy.detach()) / samples
            if not all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()):
                raise RuntimeError("Nonfinite streaming gradient")
            norm = float(torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0))
            optimizer.step()
            row = dict(group=group + 1, root=root["id"], family=root["family"], generation=root["generation"],
                       sampled_transitions=samples, terminal_advantages=advantage, task_loss=task_loss, entropy=entropy_sum,
                       gradient_norm=norm, inference_seconds=result["inference_seconds"], update_seconds=time.perf_counter() - update_start,
                       summaries=result["summaries"], audits=result["audits"], artifacts=result["artifacts"], elapsed_seconds=time.perf_counter() - started)
            groups.append(row)
            write(output / f"group-{group + 1:04d}.json", row)
            print(json.dumps(dict(stage="streaming-optimizer-update", group=group + 1, root=root["id"], informative=any(advantage),
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
                        changed_weight_elements=sum(int(torch.count_nonzero(v.detach() - before[k])) for k, v in model.state_dict().items()),
                        sampled_transitions=sum(g["sampled_transitions"] for g in groups),
                        dynamic_groups=sum(g["generation"] == "dynamic-catalog" for g in groups),
                        unique_dynamic_roots=len({g["root"] for g in groups if g["generation"] == "dynamic-catalog"}))
        write(output / "training.json", training)
        roots = [generate("validation") for _ in range(config["evaluation_roots"])] + [target]
        write(output / "evaluation-roots.json", roots)
        for horizon in config["evaluation_horizons"]:
            for seed in config["evaluation_seeds"]:
                for name, policy in [("initial", restore(initial)), ("uniform", None), ("trained", model)]:
                    for start in range(0, len(roots), config["workers"]):
                        batch = roots[start:start + config["workers"]]
                        result = rollout(pool, policy, batch, horizon, config, seed, uniform=name == "uniform")
                        for i, root in enumerate(batch):
                            row = dict(model=name, root=root["id"], family=root["family"], split=root["split"], horizon=horizon, seed=seed,
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
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="streaming-training-failed", severity="error", stage="training", message=str(error), entity=None)), flush=True)
        raise SystemExit(1)
