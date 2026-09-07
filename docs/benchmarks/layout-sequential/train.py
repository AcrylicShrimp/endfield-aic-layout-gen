"""Online expanded-action policy gradients followed by matched initial-weight controls."""

import argparse
import hashlib
import json
from pathlib import Path
import random
import sys
import time

import torch

from client import Pool

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "layout-rl"))
from policy import Policy, checkpoint, distribution, restore, terminal_advantages


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate(config):
    if config.get("schema_version") != 1:
        raise ValueError("Unsupported sequential training schema")
    for key, limit in (("groups", 10000), ("workers", 16), ("horizon", 16384), ("threads", 16),
                       ("width", 256), ("target_interval", 10000), ("episode_ms", 300000)):
        if type(config.get(key)) is not int or not 1 <= config[key] <= limit:
            raise ValueError(f"Invalid {key}")
    if config["workers"] < 2 or config["width"] % 2 or type(config.get("seed")) is not int:
        raise ValueError("Expected multiple trajectories, even model width and integer seed")
    if not 0 < config.get("learning_rate", 0) <= .1 or not 0 <= config.get("entropy", -1) <= .1:
        raise ValueError("Invalid optimizer configuration")
    if not config.get("evaluation_horizons") or any(type(n) is not int or not 1 <= n <= 16384 for n in config["evaluation_horizons"]):
        raise ValueError("Invalid evaluation horizons")
    if not config.get("evaluation_seeds") or any(type(n) is not int for n in config["evaluation_seeds"]):
        raise ValueError("Invalid evaluation seeds")


def advance(pool, obs, choices):
    futures = [(i, pool.executor.submit(w.call, "step", action=a))
               for i, (w, o, a) in enumerate(zip(pool.workers, obs, choices)) if not o["done"]]
    for i, future in futures:
        obs[i] = future.result()
    return obs


def evaluate(pool, models, roots, config, output):
    rows = []
    for horizon in config["evaluation_horizons"]:
        for seed in config["evaluation_seeds"]:
            for name, model in models:
                # Identical batch/root order and RNG seed for each model control.
                torch.manual_seed(seed)
                for start in range(0, len(roots), len(pool.workers)):
                    batch = roots[start:start + len(pool.workers)]
                    futures = [pool.executor.submit(w.call, "reset", root=r["id"], budget=dict(steps=horizon, elapsed_ms=config["episode_ms"]))
                               for w, r in zip(pool.workers, batch)]
                    obs = [f.result() for f in futures]
                    while any(not o["done"] for o in obs):
                        with torch.no_grad():
                            choices = distribution(model, obs).sample().tolist()
                        advance(pool, obs, choices)
                    for w, root in zip(pool.workers, batch):
                        summary = w.call("summary")
                        audit = w.call("audit")
                        artifact = w.call("save")
                        row = dict(model=name, root=root["id"], split=root["split"], horizon=horizon, seed=seed,
                                   summary=summary, audit=audit, artifact=artifact)
                        rows.append(row)
                        write(output / f"evaluation-{len(rows):04d}.json", row)
                        print(json.dumps(dict(stage="evaluation", model=name, root=root["id"], horizon=horizon,
                                              proposals=summary["work"]["proposals"], best=summary["best_score"]["tuple"],
                                              termination=summary["termination"])), flush=True)
    return rows


def run(options):
    config = read(options.config)
    validate(config)
    output = Path(options.output_directory).resolve()
    output.mkdir()
    sources = [Path(__file__), Path(__file__).with_name("client.py"),
               Path(__file__).parent.parent / "layout-rl" / "policy.py",
               Path(__file__).parent.parent / "layout-learning" / "model.py", Path(options.roots), Path(options.config)]
    provenance = [dict(path=str(p.resolve()), sha256=sha(p)) for p in sources]
    write(output / "provenance.json", provenance)
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.manual_seed(config["seed"])
    randomizer = random.Random(config["seed"])
    began = time.perf_counter()
    groups = []
    with Pool(Path(options.roots).resolve(), output, config["workers"]) as pool:
        train_roots = [r for r in pool.metadata["roots"] if r["split"] == "train" and r["id"] != "heavy-strong"]
        target = next(r for r in pool.metadata["roots"] if r["id"] == "heavy-strong")
        validation = [r for r in pool.metadata["roots"] if r["split"] == "validation"] + [target]
        if not train_roots or len(validation) < 2 or any(r["split"] == "test" for r in pool.metadata["roots"]):
            raise ValueError("Invalid non-test root partitions")
        model = Policy(len(pool.metadata["token_fields"]), len(pool.metadata["action_fields"]), width=config["width"], architecture="graph")
        initial_checkpoint = checkpoint(model)
        initial_weights = {k: v.detach().clone() for k, v in model.state_dict().items()}
        write(output / "initial-model.json", initial_checkpoint)
        optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
        print(json.dumps(dict(stage="training-started", groups=config["groups"], horizon=config["horizon"],
                              training_roots=len(train_roots) + 1, parameters=sum(p.numel() for p in model.parameters()))), flush=True)
        for group in range(config["groups"]):
            start = time.perf_counter()
            root = target if (group + 1) % config["target_interval"] == 0 else randomizer.choice(train_roots)
            obs = pool.call("reset", [dict(root=root["id"], budget=dict(steps=config["horizon"], elapsed_ms=config["episode_ms"]))] * config["workers"])
            logs, entropies, masks = [], [], []
            for step in range(config["horizon"]):
                active = torch.tensor([not o["done"] for o in obs], dtype=torch.float32)
                if not active.any():
                    break
                dist = distribution(model, obs)
                choices = dist.sample()
                logs.append(dist.log_prob(choices))
                entropies.append(dist.entropy())
                masks.append(active)
                advance(pool, obs, choices.tolist())
            if not logs:
                raise RuntimeError("Training root produced no actions within budget")
            advantage = terminal_advantages([o["best_score"] for o in obs])
            mask = torch.stack(masks)
            task_loss = -(torch.stack(logs) * advantage * mask).sum() / mask.sum()
            entropy = (torch.stack(entropies) * mask).sum() / mask.sum()
            loss = task_loss - config["entropy"] * entropy
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            if not all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()):
                raise RuntimeError("Nonfinite sequential gradient")
            norm = float(torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0))
            optimizer.step()
            summaries = pool.call("summary", [{} for _ in pool.workers])
            audits = pool.call("audit", [{} for _ in pool.workers])
            artifacts = pool.call("save", [{} for _ in pool.workers])
            row = dict(group=group + 1, root=root["id"], task_loss=float(task_loss.detach()), entropy=float(entropy.detach()),
                       loss=float(loss.detach()), terminal_advantages=advantage.tolist(), gradient_norm=norm,
                       sampled_transitions=int(mask.sum()), summaries=summaries, audits=audits, artifacts=artifacts,
                       elapsed_seconds=time.perf_counter() - start)
            groups.append(row)
            write(output / f"group-{group + 1:04d}.json", row)
            print(json.dumps(dict(stage="optimizer-update", group=group + 1, root=root["id"], gradient_norm=norm,
                                  informative=bool(torch.count_nonzero(advantage)), best=[s["best_score"]["tuple"] for s in summaries],
                                  elapsed_seconds=row["elapsed_seconds"])), flush=True)
        final_checkpoint = checkpoint(model)
        restored = restore(final_checkpoint)
        with torch.no_grad():
            if not torch.equal(distribution(model, obs).logits, distribution(restored, obs).logits):
                raise RuntimeError("Checkpoint round-trip changed predictions")
        changed = sum(int(torch.count_nonzero(v.detach() - initial_weights[k])) for k, v in model.state_dict().items())
        write(output / "model.json", final_checkpoint)
        training = dict(groups=groups, optimizer_steps=len(groups), changed_weight_elements=changed,
                        informative_groups=sum(any(a != 0 for a in r["terminal_advantages"]) for r in groups),
                        sampled_transitions=sum(r["sampled_transitions"] for r in groups), training_roots=[r["id"] for r in train_roots] + [target["id"]])
        write(output / "training.json", training)
        rows = evaluate(pool, [("initial", restore(initial_checkpoint)), ("trained", model)], validation, config, output)
    for p in provenance:
        if sha(p["path"]) != p["sha256"]:
            raise RuntimeError(f'Source changed during execution: {p["path"]}')
    write(output / "report.json", dict(schema_version=1, status="complete", scope="expanded-action-online-training-pilot", test_evaluations=0,
                                      config=config, torch=torch.__version__, provenance=provenance, training=training, evaluation=rows,
                                      elapsed_seconds=time.perf_counter() - began))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--roots", required=True, help="Expanded sequential environment plan")
    parser.add_argument("--config", required=True)
    parser.add_argument("--output-directory", required=True)
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="sequential-training-failed", severity="error", stage="training", message=str(error), entity=None)), flush=True)
        raise SystemExit(1)
