"""Isolate environment parity from CUDA update repeatability on one fixed rollout."""

import argparse
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path

import numpy as np
import torch

from evidence import provenance, write
from native import Runtime, layout
from native_training_comparison import equal, policy_hash
from policy import Policy, update
from reference.acceptance import Catalog
from reference.environment import Environment, serialize


def collect(runtime, config, saved):
    native = runtime.new_engine()
    reference = Environment(Catalog(runtime.root, runtime.path), config)
    example, _ = native.observe()
    actor = Policy(example)
    actor.load_state_dict(saved["policy"])
    generators = [torch.Generator().set_state(saved["torch_rng"]) for _ in range(2)]
    rollout = {key: [] for key in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
    observation_digest = hashlib.sha256()
    for index in range(config["rollout_commands"]):
        actual, handle = native.observe()
        candidates = reference.candidates()
        expected = reference.observe(candidates)
        if json.loads(native.export_candidates_json(handle)) != [command.to_dict() for command in candidates.commands]:
            raise ValueError(f"learning_parity.candidates: index {index}")
        for key, value in actual.items():
            wanted = expected[key]
            if value.dtype != wanted.dtype or value.shape != wanted.shape or value.tobytes() != wanted.tobytes():
                raise ValueError(f"learning_parity.observation: index {index}: {key}")
            observation_digest.update(key.encode())
            observation_digest.update(str(value.dtype).encode())
            observation_digest.update(str(value.shape).encode())
            observation_digest.update(value.tobytes())
        action, log_prob, value = actor.act(actual, generators[0])
        if (action, log_prob, value) != actor.act(expected, generators[1]):
            raise ValueError(f"learning_parity.inference: index {index}")
        if not torch.equal(generators[0].get_state(), generators[1].get_state()):
            raise ValueError(f"learning_parity.sampling_rng: index {index}")
        reward, done, outcome = native.step(handle, action)
        if (reward, done, outcome) != reference.step(candidates.commands[action].to_dict()):
            raise ValueError(f"learning_parity.transition: index {index}")
        for field in ("draft", "base", "best"):
            if layout(native, field) != serialize(getattr(reference, field)):
                raise ValueError(f"learning_parity.state: index {index}: {field}")
        for key, item in (("observations", actual), ("actions", action), ("log_probs", log_prob),
                          ("values", value), ("rewards", reward), ("dones", done)):
            rollout[key].append(item)
        if done:
            native.reset()
            reference.reset()
    return rollout, generators[0].get_state(), observation_digest.hexdigest()


def repeat_updates(rollout, saved, config, sampling_rng, deterministic, repeats):
    torch.use_deterministic_algorithms(deterministic)
    torch.backends.cudnn.deterministic = deterministic
    outputs, rows = [], []
    for repeat in range(repeats):
        model = Policy(rollout["observations"][0]).cuda()
        model.load_state_dict(saved["policy"])
        optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
        optimizer.load_state_dict(deepcopy(saved["optimizer"]))
        torch.set_rng_state(sampling_rng)
        torch.cuda.set_rng_state_all(saved["cuda_rng"])
        rng = np.random.default_rng()
        rng.bit_generator.state = deepcopy(saved["numpy_rng"])
        metrics = update(model, optimizer, rollout, config, rng)
        state = {key: value.detach().cpu().clone() for key, value in model.state_dict().items()}
        outputs.append(state)
        rows.append({"repeat": repeat, "policy_sha256": policy_hash(state),
                     "metrics": {key: value for key, value in metrics.items() if key != "seconds"},
                     "equal_first_policy": equal(outputs[0], state),
                     "maximum_parameter_difference_from_first": max(
                         (state[key]-outputs[0][key]).abs().max().item() for key in state)})
    return {"deterministic_algorithms": deterministic, "rows": rows,
            "all_policies_equal": all(row["equal_first_policy"] for row in rows)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--checkpoint", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=4)
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 2:
        parser.error("output must be fresh and repeats at least two")
    # Set before CUDA initialization, including the default-mode negative control.
    os.environ["CUBLAS_WORKSPACE_CONFIG"] = ":4096:8"
    config = json.loads(args.config.read_text())
    saved = torch.load(args.checkpoint, weights_only=False, map_location="cpu")
    if saved["commands"] != 0 or saved["config"] != config or config["device"] != "cuda":
        parser.error("requires a matching initial CUDA checkpoint")
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    runtime = Runtime(args.root, args.case, config, args.native_library)
    rollout, sampling_rng, observation_hash = collect(runtime, config, saved)
    args.output.mkdir(parents=True)
    input_path = args.output/"fixed-rollout.pt"
    torch.save({"rollout": rollout, "sampling_rng": sampling_rng}, input_path)
    modes = [repeat_updates(rollout, saved, config, sampling_rng, mode, args.repeats)
             for mode in (False, True)]
    result = {"schema_version": 1, "provenance": provenance(runtime, args.config),
              "checkpoint_sha256": hashlib.sha256(args.checkpoint.read_bytes()).hexdigest(),
              "probe_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "fixed_rollout_sha256": hashlib.sha256(input_path.read_bytes()).hexdigest(),
              "observation_sha256": observation_hash, "commands": len(rollout["actions"]),
              "parity": "Bit-exact candidate order, observations, sampled actions, log probabilities, values, RNG, transitions, and draft/base/best after every action",
              "cublas_workspace_config": os.environ["CUBLAS_WORKSPACE_CONFIG"], "modes": modes}
    write(args.output/"result.json", result)
    print(json.dumps({key: value for key, value in result.items() if key != "provenance"}, indent=2))


if __name__ == "__main__":
    main()
