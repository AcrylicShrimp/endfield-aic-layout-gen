"""Fixed-plan, sampled-action contextual-bandit experiment. Run with uv."""

import argparse
import hashlib
import json
import math
import platform
import sys
import time
from pathlib import Path

import torch
from model import Policy, checkpoint, fit_normalization, pack, restore


def read(p):
    return json.loads(Path(p).read_text())


def write(p, value):
    with Path(p).open("x") as f:
        json.dump(value, f, allow_nan=False, separators=(",", ":"))
        f.write("\n")


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def validate_data(data):
    if data.get("schema_version") != 1 or data.get("factory_validated") is not False:
        raise ValueError("Expected schema-1, internally validated fragment dataset")
    fingerprints, ids = set(), set()
    action_count = None
    for s in data["samples"]:
        if s["fingerprint"] in fingerprints or s["id"] in ids:
            raise ValueError("Duplicate state fingerprint or sample ID")
        fingerprints.add(s["fingerprint"])
        ids.add(s["id"])
        f = s["features"]
        if set(f) != {"tokens", "adjacency", "actions"}:
            raise ValueError("Unexpected model feature key")
        n, k = len(f["tokens"]), len(f["actions"])
        if action_count is None:
            action_count = k
        if not 1 <= n <= 256 or k != action_count or k < 2:
            raise ValueError("Invalid token count or inconsistent action count")
        for field, dim in (("tokens", len(data["token_fields"])),
                           ("actions", len(data["action_fields"])), ("adjacency", n)):
            rows = f[field]
            if len(rows) != (k if field == "actions" else n):
                raise ValueError("Invalid feature row count")
            if any(len(row) != dim or any(not isinstance(v, (int, float)) or not math.isfinite(v)
                                         for v in row) for row in rows):
                raise ValueError("Nonfinite or malformed feature row")
        if any(v not in (0, 1) for row in f["adjacency"] for v in row):
            raise ValueError("Adjacency must be binary")
        if len(s["rewards"]) != k or len(s["outcomes"]) != k or len(s["actions"]) != k:
            raise ValueError("Outcome/action count mismatch")
        if not 0 <= s["current"] < k or not s["outcomes"][s["current"]]["accepted"]:
            raise ValueError("Current action is not validated")
        scores = sorted({tuple(o["score"]["tuple"]) for o in s["outcomes"] if o["accepted"]})
        if len(scores) < 2:
            raise ValueError("Dataset task is not informative")
        expected = [1 - 0.5 * scores.index(tuple(o["score"]["tuple"])) / (len(scores) - 1)
                    if o["accepted"] else 0 for o in s["outcomes"]]
        if any(abs(a - b) > 1e-12 for a, b in zip(expected, s["rewards"])):
            raise ValueError("Reward does not match verified lexicographic order")
    if not all(any(s["split"] == split for s in data["samples"])
               for split in ("train", "validation", "test", "out-of-family")):
        raise ValueError("Missing experiment split")


def metrics(samples, choices=None):
    """Uniform baseline is analytic; all other policies use selected actions."""
    sums = dict(proposal_validity=0.0, best_hit=0.0, mean_reward=0.0, safe_improvement=0.0)
    by_request = {}
    for row, s in enumerate(samples):
        current = s["outcomes"][s["current"]]["score"]
        selected = [(choices[row], 1.0)] if choices is not None else [(i, 1 / len(s["outcomes"])) for i in range(len(s["outcomes"]))]
        group = by_request.setdefault(s["request"], {"states": 0, "current_area": 0, "applied_area": 0,
                                                     "applied_tiles": 0, "applied_turns": 0})
        group["states"] += 1
        group["current_area"] += current["area"]
        for i, weight in selected:
            o, reward = s["outcomes"][i], s["rewards"][i]
            improve = o["accepted"] and tuple(o["score"]["tuple"]) < tuple(current["tuple"])
            applied = o["score"] if improve else current
            for key, val in (("proposal_validity", o["accepted"]), ("best_hit", reward == 1),
                             ("mean_reward", reward), ("safe_improvement", improve)):
                sums[key] += val * weight
            group["applied_area"] += applied["area"] * weight
            group["applied_tiles"] += applied["physical_tiles"] * weight
            group["applied_turns"] += applied["turns"] * weight
    result = {k: v / len(samples) for k, v in sums.items()}
    result["mean_regret"] = 1 - result["mean_reward"]
    result["states"] = len(samples)
    result["by_request"] = {k: {field: val if field == "states" else val / group["states"]
                                  for field, val in group.items()} for k, group in by_request.items()}
    return result


def screened_choice(sample):
    """Cheap geometric control: reject obvious collisions, but never run a router."""
    base, actions = sample["base"], sample["actions"]
    target = base["placements"][-1]
    def body(p):
        return {(x, y) for x in range(p["x"], p["x"] + p["width"])
                for y in range(p["y"], p["y"] + p["height"])}
    bodies = set().union(*(body(p) for p in base["placements"]))
    cells = {(c["x"], c["y"]) for n in base["transport_networks"] for c in n["cells"]}
    candidates = []
    for i, (action, f) in enumerate(zip(actions, sample["features"]["actions"])):
        proposed = body(action["pose"])
        if proposed & (bodies | cells):
            continue
        endpoints = [(int(f[x]) + target["x"], int(f[y]) + target["y"]) for x, y in ((8, 9), (14, 15))]
        kind = "belt" if f[18] else "pipe"
        layer = {(c["x"], c["y"]) for n in base["transport_networks"] if n["transport"] == kind for c in n["cells"]}
        if any(p in bodies | proposed | layer for p in endpoints):
            continue
        points = bodies | proposed | cells | set(endpoints)
        area = (max(p[0] for p in points) - min(p[0] for p in points) + 1) * (max(p[1] for p in points) - min(p[1] for p in points) + 1)
        distance = abs(endpoints[0][0] - endpoints[1][0]) + abs(endpoints[0][1] - endpoints[1][1])
        candidates.append((area, distance, i))
    return min(candidates)[2] if candidates else sample["current"]


@torch.no_grad()
def predict(model, inputs):
    model.eval()
    # Keep diagnostic evaluation memory bounded independently of total dataset size.
    return torch.cat([model(*(t[i:i + 64] for t in inputs)).argmax(-1)
                      for i in range(0, len(inputs[0]), 64)]).cpu().tolist()


def sync(device):
    if device == "cuda":
        torch.cuda.synchronize()


@torch.no_grad()
def inference_timing(model, samples, norm, device, repeats):
    result = {}
    for n in (1, min(64, len(samples))):
        inputs = pack(samples[:n], norm, device)
        for _ in range(10):
            model(*inputs)
        sync(device)
        timings = []
        for _ in range(repeats):
            start = time.perf_counter()
            model(*inputs)
            sync(device)
            timings.append((time.perf_counter() - start) * 1000)
        timings.sort()
        result[str(n)] = {"batch_size": n, "p50_ms": timings[len(timings) // 2],
                          "p95_ms": timings[min(len(timings) - 1, math.ceil(.95 * len(timings)) - 1)],
                          "scope": "preencoded tensors; excludes features, candidate generation and routing"}
    return result


def train_one(config, setting, seed, splits, encoded, normalization, device, output):
    torch.manual_seed(seed)
    model = Policy(len(normalization["tokens"]["mean"]), len(normalization["actions"]["mean"]),
                   config["width"], setting["architecture"]).to(device)
    initial = {k: v.detach().clone() for k, v in model.state_dict().items()}
    untrained = {split: metrics(rows, predict(model, encoded[split])) for split, rows in splits.items()}
    optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"], weight_decay=0)
    batch_rng = torch.Generator(device=device).manual_seed(seed + 1000)
    action_rng = torch.Generator(device=device).manual_seed(seed + 2000)
    true_rewards = torch.tensor([s["rewards"] for s in splits["train"]], dtype=torch.float32, device=device)
    signals = true_rewards.clone()
    if setting["signal"] == "shuffled":
        shuffle_rng = torch.Generator(device=device).manual_seed(seed + 3000)
        for i in range(len(signals)):
            signals[i] = signals[i, torch.randperm(signals.shape[1], generator=shuffle_rng, device=device)]
    elif setting["signal"] == "zero":
        signals.zero_()
    baseline, curves, observed, seconds, gradient_norm = 0.0, [], set(), 0.0, 0.0
    def curve(step, loss=None):
        return {"step": step, "loss": loss, "last_gradient_norm": gradient_norm,
                **{s: metrics(splits[s], predict(model, encoded[s])) for s in ("train", "validation")}}
    curves.append(curve(0))
    for step in range(1, config["steps"] + 1):
        sync(device)
        start = time.perf_counter()
        model.train()
        indices = torch.randint(len(splits["train"]), (config["batch_size"],), generator=batch_rng, device=device)
        inputs = tuple(t[indices] for t in encoded["train"])
        logits = model(*inputs)
        log_prob = logits.log_softmax(-1)
        if setting["signal"] == "supervised":
            targets = (signals[indices] == signals[indices].max(-1, keepdim=True).values).float()
            targets /= targets.sum(-1, keepdim=True)
            loss = -(targets * log_prob).sum(-1).mean()
        else:
            chosen = torch.multinomial(logits.softmax(-1), 1, generator=action_rng).squeeze(-1)
            # Only sampled labels influence this gradient/baseline, not the full outcome table.
            reward = signals[indices, chosen]
            loss = -(log_prob.gather(1, chosen[:, None]).squeeze(1) * (reward - baseline)).mean()
            baseline = .95 * baseline + .05 * reward.mean().item()
            observed.update(zip(indices.cpu().tolist(), chosen.cpu().tolist()))
        optimizer.zero_grad(set_to_none=True)
        loss.backward()
        if not torch.isfinite(loss) or any(p.grad is not None and not torch.isfinite(p.grad).all() for p in model.parameters()):
            raise ValueError(f"Nonfinite loss/gradient at seed={seed}, step={step}, setting={setting}")
        gradient_norm = float(torch.nn.utils.clip_grad_norm_(model.parameters(), 5))
        optimizer.step()
        sync(device)
        seconds += time.perf_counter() - start
        if step % config["curve_interval"] == 0 or step == config["steps"]:
            curves.append(curve(step, loss.item()))
    changed = sum(int(torch.count_nonzero(v - initial[k])) for k, v in model.state_dict().items())
    if setting["signal"] == "zero" and changed:
        raise ValueError("Zero-reward control changed weights")
    name = f'{setting["architecture"]}-{setting["signal"]}-{seed}'
    checkpoint_path = output / f"{name}.json"
    write(checkpoint_path, checkpoint(model, normalization))
    loaded = restore(read(checkpoint_path), device).eval()
    with torch.no_grad():
        before = model(*(t[:4] for t in encoded["validation"]))
        after = loaded(*(t[:4] for t in encoded["validation"]))
        serialization_error = (before - after).abs().max().item()
    if serialization_error != 0:
        raise ValueError("JSON checkpoint changed policy output")
    choices = {s: predict(model, encoded[s]) for s in splits}
    params = sum(p.numel() for p in model.parameters())
    result = {"name": name, **setting, "seed": seed, "parameters": params, "parameter_bytes_float32": params * 4,
              "changed_weight_elements": changed, "training_seconds": seconds,
              "ms_per_update": seconds * 1000 / config["steps"],
              "cached_samples_per_second": config["steps"] * config["batch_size"] / seconds,
              "sampled_reward_observations": 0 if setting["signal"] == "supervised" else config["steps"] * config["batch_size"],
              "unique_sampled_pairs": len(observed), "all_counterfactual_labels_used": setting["signal"] == "supervised",
              "supervised_label_reads": config["steps"] * config["batch_size"] * signals.shape[1] if setting["signal"] == "supervised" else 0,
              "untrained": untrained, "metrics": {s: metrics(rows, choices[s]) for s, rows in splits.items()},
              "curves": curves, "choices": {s: [{"sample": row["id"], "action": i} for row, i in zip(splits[s], choices[s])] for s in ("test", "out-of-family")},
              "inference": inference_timing(model, splits["test"], normalization, device, config["inference_repeats"]),
              "checkpoint": {"file": checkpoint_path.name, "sha256": sha(checkpoint_path), "bytes": checkpoint_path.stat().st_size,
                             "max_roundtrip_logit_error": serialization_error}}
    print(json.dumps({"run": name, "training_seconds": seconds, "test": result["metrics"]["test"]}), flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-directory", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--output-directory", type=Path, required=True)
    parser.add_argument("--device", choices=("cpu", "cuda"), default="cpu")
    options = parser.parse_args()
    started = time.perf_counter()
    config, directory = read(options.config), options.dataset_directory.resolve()
    if config.get("schema_version") != 1 or not config.get("seeds") or len(set(config["seeds"])) != len(config["seeds"]) or any(not isinstance(s, int) for s in config["seeds"]):
        raise ValueError(f"Invalid training seeds/schema: {options.config}")
    for key in ("steps", "batch_size", "width", "threads", "curve_interval", "inference_repeats"):
        if not isinstance(config[key], int) or config[key] < 1:
            raise ValueError(f"Invalid positive training option {key}: {options.config}")
    if not 0 < config["learning_rate"] < 1 or not config["runs"] or any(r["architecture"] not in ("graph", "pooled") or r["signal"] not in ("normal", "shuffled", "zero", "supervised") for r in config["runs"]):
        raise ValueError(f"Invalid training runs/rate: {options.config}")
    if options.device == "cuda" and not torch.cuda.is_available():
        raise ValueError("CUDA was explicitly requested but is unavailable; no CPU fallback")
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    generation = read(directory / "generation.json")
    if generation["status"] != "complete" or sha(directory / "dataset.json") != generation["dataset_sha256"] or sha(directory / "provenance.json") != generation["provenance_sha256"]:
        raise ValueError(f"Incomplete or changed dataset: {directory}")
    provenance = read(directory / "provenance.json")
    for p in provenance:
        if sha(p["path"]) != p["sha256"]:
            raise ValueError(f'Stale environment provenance: {p["path"]}')
    data = read(directory / "dataset.json")
    validate_data(data)
    splits = {split: [s for s in data["samples"] if s["split"] == split] for split in ("train", "validation", "test", "out-of-family")}
    original_train_count = len(splits["train"])
    train_limit = config.get("train_limit", original_train_count)
    if not isinstance(train_limit, int) or not 1 <= train_limit <= original_train_count:
        raise ValueError(f"Invalid train_limit: {options.config}")
    splits["train"] = splits["train"][:train_limit]
    normalization = fit_normalization(splits["train"])
    encoded = {s: pack(rows, normalization, options.device) for s, rows in splits.items()}
    output = options.output_directory.resolve()
    output.mkdir()
    implementation = [Path(__file__), Path(__file__).with_name("model.py"), options.config,
                      Path(__file__).with_name("uv.lock"), Path(__file__).with_name("pyproject.toml")]
    provenance = [{"path": str(p.resolve()), "sha256": sha(p)} for p in implementation]
    write(output / "config.json", config)
    write(output / "provenance.json", provenance)
    baselines = {}
    baseline_started = time.perf_counter()
    for name in ("uniform", "retain", "cheap", "screened-geometry", "candidate-table-oracle"):
        baselines[name] = {}
        for split, rows in splits.items():
            choices = None if name == "uniform" else [s["current"] if name == "retain" else s["cheap_choice"] if name == "cheap" else screened_choice(s) if name == "screened-geometry" else s["rewards"].index(1) for s in rows]
            baselines[name][split] = metrics(rows, choices)
    baseline_seconds = time.perf_counter() - baseline_started
    screened_selections = {s: [{"sample": row["id"], "action": screened_choice(row)} for row in splits[s]] for s in ("test", "out-of-family")}
    runs = [train_one(config, r, seed, splits, encoded, normalization, options.device, output)
            for r in config["runs"] for seed in config["seeds"]]
    for p in provenance:
        if sha(p["path"]) != p["sha256"]:
            raise ValueError(f'Training implementation changed during run: {p["path"]}')
    report = {"schema_version": 1, "status": "complete", "factory_validated": False,
              "experiment": "one-step cached contextual bandit, not online factory RL",
              "dataset_directory": str(directory), "dataset_sha256": generation["dataset_sha256"],
              "device": options.device, "threads": config["threads"], "python": platform.python_version(),
              "training_states_used": train_limit, "training_states_available": original_train_count,
              "baseline_evaluation_seconds": baseline_seconds, "screened_choices": screened_selections,
              "torch": torch.__version__, "platform": platform.platform(), "machine": platform.machine(),
              "total_elapsed_seconds": time.perf_counter() - started, "baselines": baselines, "runs": runs}
    write(output / "training.json", report)
    print(json.dumps({"status": "complete", "output": str(output), "total_elapsed_seconds": report["total_elapsed_seconds"]}))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(json.dumps({"code": "learning-training-failed", "severity": "error", "stage": "training",
                          "message": str(error), "entity": sys.argv[1:]}), file=sys.stderr)
        sys.exit(1)
