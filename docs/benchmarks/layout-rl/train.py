"""Online full-factory trajectories with terminal-comparison Monte Carlo policy gradients."""

import argparse
import hashlib
import json
from pathlib import Path
import random
import time

import torch

from client import Pool
from policy import Policy, checkpoint, distribution, restore, terminal_advantages


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def local_advantages(transitions):
    values = []
    for transition in transitions:
        if not transition["accepted"]:
            values.append(-1.0)
        else:
            before, after = tuple(transition["before"]), tuple(transition["score"])
            values.append(float((before > after) - (before < after)))
    values = torch.tensor(values)
    return values - values.mean()


def validate_config(config):
    if config.get("schema_version") != 1:
        raise ValueError("Unsupported training config schema")
    for key, maximum in (("groups", 10000), ("workers", 16), ("horizon", 1024), ("threads", 16),
                         ("width", 256), ("curve_interval", 10000)):
        if type(config.get(key)) is not int or not 1 <= config[key] <= maximum:
            raise ValueError(f"Invalid {key}")
    if config["workers"] < 2 or config["width"] % 2 or config.get("architecture") not in ("graph", "pooled"):
        raise ValueError("Expected multiple trajectories and an even-width graph/pooled model")
    if not 0 < config.get("learning_rate", 0) <= .1 or not config.get("runs"):
        raise ValueError("Invalid learning rate or empty run list")
    for run in config["runs"]:
        if run.get("signal") not in ("terminal", "myopic", "shuffled", "zero") or type(run.get("seed")) is not int:
            raise ValueError("Invalid signal/seed")


def evaluate_validation(pool, model, roots, horizon):
    rows = []
    for start in range(0, len(roots), len(pool.workers)):
        batch = roots[start:start + len(pool.workers)]
        workers = pool.workers[:len(batch)]
        futures = [pool.executor.submit(w.call, "reset", root=r["id"], budget=dict(steps=horizon, elapsed_ms=30000))
                   for w, r in zip(workers, batch)]
        obs = [f.result() for f in futures]
        while any(not o["done"] for o in obs):
            with torch.no_grad():
                choices = distribution(model, obs).logits.argmax(-1).tolist()
            futures = [(i, pool.executor.submit(w.call, "step", action=a))
                       for i, (w, o, a) in enumerate(zip(workers, obs, choices)) if not o["done"]]
            for i, future in futures:
                obs[i] = future.result()
        for w in workers:
            rows.append(w.call("summary"))
    return rows


def train_run(options, config, run, index):
    name = f'{index:02d}-{run["signal"]}-{run["seed"]}'
    output = Path(options.output_directory).resolve() / name
    output.mkdir()
    randomizer = random.Random(run["seed"])
    shuffler = random.Random(run["seed"] + 9000)
    torch.manual_seed(run["seed"])
    began = time.perf_counter()
    unique_pairs = set()
    with Pool(Path(options.roots).resolve(), output, config["workers"]) as pool:
        train_roots = [r for r in pool.metadata["roots"] if r["split"] == "train"]
        validation_roots = [r for r in pool.metadata["roots"] if r["split"] == "validation"]
        if config.get("training_roots") is not None:
            requested = config["training_roots"]
            if not isinstance(requested, list) or not requested or len(set(requested)) != len(requested) or any(name not in {r["id"] for r in train_roots} for name in requested):
                raise ValueError("training_roots must name distinct imported training roots")
            train_roots = [r for r in train_roots if r["id"] in requested]
        if not train_roots or not validation_roots:
            raise ValueError("Training and validation roots are required")
        model = Policy(len(pool.metadata["token_fields"]), len(pool.metadata["action_fields"]),
                       width=config["width"], architecture=config["architecture"])
        initial_weights = {k: v.detach().clone() for k, v in model.state_dict().items()}
        optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
        groups, curves = [], []
        initial_validation = evaluate_validation(pool, model, validation_roots, config["horizon"])
        curves.append(dict(group=0, validation=initial_validation))
        for group in range(config["groups"]):
            group_start = time.perf_counter()
            root = randomizer.choice(train_roots)
            obs = pool.call("reset", [dict(root=root["id"], budget=dict(steps=config["horizon"], elapsed_ms=30000))] * config["workers"])
            log_probabilities, local_signals = [], []
            inference_seconds = 0.0
            for step in range(config["horizon"]):
                start = time.perf_counter()
                dist = distribution(model, obs)
                chosen = dist.sample()
                log_probabilities.append(dist.log_prob(chosen))
                inference_seconds += time.perf_counter() - start
                obs = pool.call("step", [dict(action=a) for a in chosen.tolist()])
                transitions = [o["transition"] for o in obs]
                if any(t.get("performed") is False for t in transitions):
                    raise RuntimeError("Training episode hit its wall-time safety limit")
                local_signals.append(local_advantages(transitions))
                for transition in transitions:
                    unique_pairs.add((transition["parent_sha256"], json.dumps(transition["action"], sort_keys=True)))
            advantage = terminal_advantages([o["best_score"] for o in obs])
            if run["signal"] == "shuffled":
                order = list(range(config["workers"]))
                shuffler.shuffle(order)
                signal = advantage[order].expand(config["horizon"], -1)
            elif run["signal"] == "zero":
                signal = torch.zeros(config["horizon"], config["workers"])
            elif run["signal"] == "myopic":
                signal = torch.stack(local_signals)
            else:
                signal = advantage.expand(config["horizon"], -1)
            start = time.perf_counter()
            loss = -(torch.stack(log_probabilities) * signal).mean()
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            if not all(p.grad is None or torch.isfinite(p.grad).all() for p in model.parameters()):
                raise RuntimeError("Nonfinite gradient")
            gradient_norm = float(torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0))
            optimizer.step()
            update_seconds = time.perf_counter() - start
            summaries = pool.call("summary", [{} for _ in pool.workers])
            artifacts = pool.call("save", [{} for _ in pool.workers])
            groups.append(dict(group=group + 1, root=root["id"], loss=float(loss.detach()), terminal_advantages=advantage.tolist(),
                               signal_nonzero=int(torch.count_nonzero(signal)), gradient_norm=gradient_norm,
                               inference_seconds=inference_seconds, update_seconds=update_seconds,
                               elapsed_seconds=time.perf_counter() - group_start, summaries=summaries, artifacts=artifacts))
            if (group + 1) % config["curve_interval"] == 0 or group + 1 == config["groups"]:
                validation = evaluate_validation(pool, model, validation_roots, config["horizon"])
                curves.append(dict(group=group + 1, validation=validation))
                print(json.dumps(dict(run=name, group=group + 1, groups=config["groups"],
                                      informative_groups=sum(any(a != 0 for a in g["terminal_advantages"]) for g in groups),
                                      validation=[r["best_score"]["tuple"] for r in validation],
                                      elapsed_seconds=time.perf_counter() - began)), flush=True)
        weights = checkpoint(model)
        restored = restore(weights)
        with torch.no_grad():
            if not torch.equal(distribution(model, obs).logits, distribution(restored, obs).logits):
                raise RuntimeError("JSON checkpoint round-trip changed predictions")
        changed = sum(int(torch.count_nonzero(v.detach() - initial_weights[k])) for k, v in model.state_dict().items())
        if run["signal"] == "zero" and changed:
            raise RuntimeError("Zero-signal control changed weights")
        write(output / "model.json", weights)
        result = dict(schema_version=1, name=name, signal=run["signal"], seed=run["seed"], parameters=sum(p.numel() for p in model.parameters()),
                      training_roots=[r["id"] for r in train_roots],
                      changed_weight_elements=changed, groups=groups, curves=curves, unique_sampled_state_actions=len(unique_pairs),
                      sampled_transitions=config["groups"] * config["workers"] * config["horizon"], elapsed_seconds=time.perf_counter() - began,
                      checkpoint=dict(path=str(output / "model.json"), sha256=sha(output / "model.json")))
        write(output / "training.json", result)
    return result


def run(options):
    config = read(options.config)
    validate_config(config)
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    output = Path(options.output_directory).resolve()
    output.mkdir()
    sources = [Path(__file__), Path(__file__).with_name("client.py"), Path(__file__).with_name("policy.py"),
               Path(__file__).parent.parent / "layout-learning" / "model.py", Path(options.roots), Path(options.config)]
    provenance = [dict(path=str(p.resolve()), sha256=sha(p)) for p in sources]
    write(output / "provenance.json", provenance)
    began = time.perf_counter()
    runs = [train_run(options, config, setting, i) for i, setting in enumerate(config["runs"])]
    for source in provenance:
        if sha(source["path"]) != source["sha256"]:
            raise RuntimeError(f'Source changed during training: {source["path"]}')
    write(output / "report.json", dict(schema_version=1, status="complete", device="cpu", threads=config["threads"],
                                        torch=torch.__version__, config=config, roots=str(Path(options.roots).resolve()),
                                        elapsed_seconds=time.perf_counter() - began,
                                        runs=[{k: v for k, v in r.items() if k != "groups"} for r in runs], provenance=provenance))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--roots", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--output-directory", required=True)
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="rl-training-failed", severity="error", stage="training", message=str(error), entity=None)), flush=True)
        raise SystemExit(1)
