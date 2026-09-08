"""Read frozen policies and reconstruct first-rollout learning signals without training."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

import numpy as np
import torch

from curriculum import Collector, prefixes
from evidence import write
from native import Runtime
from native_training_comparison import rows
from policy import Policy, advantages, batch, single


def describe(values):
    a = np.asarray(values, dtype=float)
    return {"mean": float(a.mean()), "std": float(a.std()), "min": float(a.min()), "max": float(a.max()),
            "l2": float(np.linalg.norm(a))}


def gradient_components(model, rollout, actual, critic, task, returns, config):
    """First-update actor gradient at ratio one, before clipping or optimizer changes."""
    logits, predicted = model(batch(rollout["observations"]))
    logs = logits.log_softmax(-1)
    selected = logs.gather(1, torch.tensor(rollout["actions"])[:, None]).squeeze(1)
    denominator = actual.std() + 1e-8
    losses = {name: -(selected * torch.from_numpy((component-component.mean())/denominator)).mean()
              for name, component in (("reward", task), ("critic", critic))}
    losses["entropy"] = config["entropy_coefficient"] * (logs.exp()*logs).sum(-1).mean()
    losses["value"] = config["value_coefficient"] * (predicted-torch.from_numpy(returns)).square().mean()
    parameters = list(model.named_parameters())
    result = {}
    for name, loss in losses.items():
        gradients = torch.autograd.grad(loss, [p for _, p in parameters], retain_graph=True, allow_unused=True)
        result[name] = {"all_parameters_l2": float(torch.sqrt(sum(g.square().sum() for g in gradients if g is not None))),
                        "action_scorer_l2": float(torch.sqrt(sum(g.square().sum() for (key, _), g in zip(parameters, gradients)
                                                                 if g is not None and key.startswith("scorer.")))) if name != "value" else 0.0}
    return result


def first_choice_reachability(collector, runtimes, streams):
    """Exhaust only the next actual action domain after a sampled first action."""
    results = []
    for key, stream in zip(collector.keys, streams):
        env = runtimes[key].new_engine()
        env.reset_with_prefix_json(json.dumps(collector.starts[key][1]))
        handle, _ = env.candidates()
        action = stream["actions"][0]
        command = json.loads(env.export_candidates_json(handle))[action]
        reward, _, _ = env.step(handle, action)
        after = env.export_state_json()
        _, count = env.candidates()
        improving = 0
        if reward == 0:
            for candidate in range(count):
                env.restore_state_json(after)
                handle, _ = env.candidates()
                improving += env.step(handle, candidate)[0] > 0
        results.append({"case": key, "first_command": command, "first_reward": reward,
                        "next_candidates": count, "improving_next_actions": improving if reward == 0 else None})
    return results


@torch.no_grad()
def inspect(model, env, desired=None):
    observation, handle = env.observe()
    domain = json.loads(env.export_candidates_json(handle))
    logits, value = model(single(observation))
    probs = logits[0].softmax(-1).numpy()
    entropy = float(-(probs * np.log(probs)).sum())
    operations = Counter()
    for command, p in zip(domain, probs):
        operations[command["op"]] += float(p)
    result = {"candidates": len(domain), "entropy": entropy, "entropy_fraction": entropy / np.log(len(domain)),
              "value": float(value[0]), "operation_probabilities": dict(operations)}
    if desired is not None:
        i = domain.index(desired)
        result.update(desired_probability=float(probs[i]), uniform_probability=1/len(domain),
                      desired_rank=1+int((probs > probs[i]).sum()), desired=desired)
    return result


def diagnose(root, matrix, library, output):
    if output.exists():
        raise ValueError(f"diagnosis.output_exists: {output}")
    output.mkdir(parents=True)
    summaries, rollout_probes = [], []
    for run in json.loads((matrix / "matrix.json").read_text())["runs"]:
        path = matrix / Path(run["directory"]).name
        metadata = json.loads((path / "metadata.json").read_text())
        config = metadata["config"]
        torch.set_num_threads(config["threads"])
        runtimes = {key: Runtime(root, root / p, config, library) for key, p in metadata["protocol"]["cases"].items()}
        starts = {key: prefixes(runtime) for key, runtime in runtimes.items()}
        for count in config["checkpoints"]:
            saved = torch.load(path / f"checkpoint-{count}.pt", map_location="cpu", weights_only=False)
            model = Policy(next(iter(runtimes.values())).new_engine().observe()[0])
            model.load_state_dict(saved["evaluation_policy"])
            model.eval()
            for key, runtime in runtimes.items():
                env = runtime.new_engine()
                for level in (0, 1, 2, 3):
                    env.reset_with_prefix_json(json.dumps(starts[key][level]))
                    desired = runtime.case["witness"][len(starts[key][level])]
                    summaries.append({"condition": metadata["condition"], "seed": metadata["seed"], "commands": count,
                                      "case": key, "remaining": level, **inspect(model, env, desired)})
            if count == 0 and metadata["condition"] in ("root-serial", "curriculum-serial"):
                collector = Collector(runtimes, config, metadata["protocol"]["streams"], metadata["seed"], False)
                try:
                    rollout, streams = collector.collect(model, 1 if metadata["condition"].startswith("curriculum") else 0)
                    expected = rows(path / "episodes.jsonl")[:len(streams)]
                    matched = all(sum(row["rewards"]) == original["return"] for row, original in zip(streams, expected))
                    if not matched:
                        raise RuntimeError(f"diagnosis.reconstruction: first rollout returns differ: {path}")
                    if any(json.loads(env.episode_json())["counts"] != original["counts"]
                           for env, original in zip(collector.envs, expected)):
                        raise RuntimeError(f"diagnosis.reconstruction: first rollout counters differ: {path}")
                    actual, returns = advantages(rollout["rewards"], rollout["values"], rollout["dones"], 0, config["gamma"], config["gae_lambda"])
                    critic, _ = advantages([0.0]*len(actual), rollout["values"], rollout["dones"], 0, config["gamma"], config["gae_lambda"])
                    task = actual - critic
                    centered = lambda a: a-a.mean()
                    rollout_probes.append({"condition": metadata["condition"], "seed": metadata["seed"],
                                           "episode_returns_reproduce": matched, "positive_steps": int(np.count_nonzero(rollout["rewards"])),
                                           "values": describe(rollout["values"]), "raw_advantage": describe(actual),
                                           "reward_component": describe(task), "critic_component": describe(critic),
                                           "centered_reward_l2": float(np.linalg.norm(centered(task))),
                                           "centered_critic_l2": float(np.linalg.norm(centered(critic))),
                                           "positive_reward_indices": [i for i,r in enumerate(rollout["rewards"]) if r>0],
                                           "reward_credit_nonzero_steps": int(np.count_nonzero(task)),
                                           "first_gradient_components": gradient_components(model, rollout, actual, critic, task, returns, config),
                                           "first_choices": first_choice_reachability(collector, runtimes, streams)
                                               if metadata["condition"] == "curriculum-serial" else [],
                                           "normalized_advantage_std": float((actual/(actual.std()+1e-8)).std())})
                finally:
                    collector.close()
    result = {"schema_version": 1, "status": "passed", "scope": "Read-only checkpoint probes and first CPU collection reconstruction; no optimizer updates",
              "policies": summaries, "first_rollouts": rollout_probes,
              "helper_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    write(output / "diagnosis.json", result)
    print(json.dumps({"status": "passed", "policies": len(summaries), "first_rollouts": rollout_probes}, indent=2))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--matrix", type=Path, required=True)
    p.add_argument("--native-library", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    a = p.parse_args()
    torch.set_num_interop_threads(1)
    diagnose(a.root.resolve(), a.matrix.resolve(), a.native_library.resolve(), a.output.resolve())
