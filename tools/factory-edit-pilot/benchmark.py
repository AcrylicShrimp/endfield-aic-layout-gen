"""Compare CPU execution settings on identical, retained on-policy rollouts."""

import argparse
from copy import deepcopy
import json
import hashlib
import os
from pathlib import Path
import statistics
import time
from types import MethodType, ModuleType
import subprocess

import numpy as np
import torch
from torch.nn import functional as F

from acceptance import Catalog
from environment import Environment
from policy import Policy, update
from run import provenance, refresh_actor, write


def unfolded_convolution(layer, spatial):
    if not torch.is_grad_enabled():
        return F.conv2d(spatial, layer.weight, layer.bias, padding=1)
    n, _, h, w = spatial.shape
    if layer.benchmark_convolution == "unfold":
        patches = F.unfold(spatial, kernel_size=3, padding=1).transpose(1, 2)
    else:
        patches = F.pad(spatial, (1, 1, 1, 1)).unfold(2, 3, 1).unfold(3, 3, 1)
        patches = patches.permute(0, 2, 3, 1, 4, 5).reshape(n, h*w, -1)
    convolved = F.linear(patches, layer.weight.flatten(1), layer.bias)
    return convolved.transpose(1, 2).reshape(n, layer.out_channels, h, w)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--checkpoint", type=Path)
    source.add_argument("--input", type=Path, help="Previously retained benchmark input.pt")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--threads", type=int, nargs="+", default=[1, 2, 4, 8])
    parser.add_argument("--actor-threads", type=int, help="CPU inference threads, defaulting to each measured update setting")
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--device", choices=("cpu", "cuda"), default="cpu", help="PPO learner device; action sampling stays on CPU")
    parser.add_argument("--reference-update", help="Trusted historical policy source for a fixed-input update comparison")
    parser.add_argument("--deterministic", action="store_true", help="Diagnose repeatability with deterministic PyTorch algorithms")
    parser.add_argument("--channels-last", action="store_true")
    parser.add_argument("--convolution", choices=("native", "unfold", "view-unfold"), default="native")
    args = parser.parse_args()
    if args.deterministic:
        os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
    torch.use_deterministic_algorithms(args.deterministic)
    torch.backends.cudnn.deterministic = args.deterministic
    if args.device == "cuda" and not torch.cuda.is_available():
        parser.error("CUDA requested but unavailable")
    if args.output.exists():
        raise ValueError(f"Refusing to overwrite benchmark {args.output}")
    if args.repeats < 1 or any(t < 1 for t in args.threads) or (args.actor_threads is not None and args.actor_threads < 1):
        parser.error("repeats and threads must be positive")
    args.output.mkdir(parents=True)
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.manual_seed(17)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    config = json.loads(args.config.read_text())
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    env = Environment(catalog, config)
    model = Policy(env.observe(env.candidates()))
    if args.convolution != "native":
        for layer in model.convolutions:
            layer.benchmark_convolution = args.convolution
            layer.forward = MethodType(unfolded_convolution, layer)
    if args.channels_last:
        model.to(memory_format=torch.channels_last)
        def channels_last(module, inputs):
            data, = inputs
            return ({**data, "raster": data["raster"].contiguous(memory_format=torch.channels_last)},)
        model.register_forward_pre_hook(channels_last)
    checkpoint = torch.load(args.input or args.checkpoint, map_location="cpu", weights_only=False)
    model.load_state_dict(checkpoint["policy"])
    state = deepcopy(model.state_dict())
    rollout = {key: [] for key in ("observations", "actions", "log_probs", "values", "rewards", "dones")}
    generator = torch.Generator().manual_seed(700017)
    for _ in range(0 if args.input else config["rollout_commands"]):
        candidates = env.candidates()
        observation = env.observe(candidates)
        action, log_prob, value = model.act(observation, generator)
        reward, done, _ = env.step(candidates.commands[action].to_dict())
        for key, value in (("observations", observation), ("actions", action), ("log_probs", log_prob),
                           ("values", value), ("rewards", reward), ("dones", done)):
            rollout[key].append(value)
        if done:
            env.reset()
    if args.input:
        rollout = checkpoint["rollout"]
    torch.save({"rollout": rollout, "policy": state, "optimizer": checkpoint["optimizer"]}, args.output/"input.pt")
    actor = deepcopy(model) if args.device == "cuda" else model
    model.to(args.device)
    variants = [("current", update)]
    reference_hash = None
    if args.reference_update:
        source = subprocess.run(["git", "show", f"{args.reference_update}:tools/factory-edit-pilot/policy.py"],
                                cwd=catalog.root, capture_output=True, text=True, check=True).stdout
        reference = ModuleType("reference_policy")
        exec(compile(source, "reference_policy.py", "exec"), reference.__dict__)
        variants.insert(0, ("reference", reference.update))
        reference_hash = hashlib.sha256(source.encode()).hexdigest()
    rows = []
    first_parameters = {}
    # Alternate order each round; the first round warms each setting and is excluded.
    for repeat in range(args.repeats+1):
        for threads in args.threads[::1 if repeat % 2 == 0 else -1]:
            for label, update_function in variants[::1 if repeat % 2 == 0 else -1]:
                torch.set_num_threads(threads)
                model.load_state_dict(state)
                optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
                optimizer.load_state_dict(deepcopy(checkpoint["optimizer"]))
                started = time.perf_counter()
                metrics = update_function(model, optimizer, rollout, config, np.random.default_rng(700017))
                elapsed = time.perf_counter()-started
                started = time.perf_counter()
                refresh_actor(model, actor)
                snapshot_seconds = time.perf_counter()-started
                torch.set_num_threads(args.actor_threads or threads)
                started = time.perf_counter()
                for observation in rollout["observations"]:
                    actor.act(observation, generator)
                row = {"threads": threads, "implementation": label, "warmup": repeat == 0, "repeat": repeat,
                       "actor_threads": args.actor_threads or threads,
                       "update_seconds": elapsed, "inference_seconds": time.perf_counter()-started,
                       "snapshot_seconds": snapshot_seconds,
                       "metrics": metrics}
                # Outside measured clocks: detect repeatability of the identical update,
                # separately for each thread setting. Retain the first result, including warm-up.
                parameters = {key: value.detach().cpu().clone() for key, value in model.state_dict().items()}
                previous = first_parameters.setdefault((threads, label), parameters)
                row["parameter_sha256"] = hashlib.sha256(b"".join(value.numpy().tobytes() for value in parameters.values())).hexdigest()
                row["max_parameter_difference_from_first"] = max(float((value-previous[key]).abs().max()) for key, value in parameters.items())
                rows.append(row)
                print(json.dumps(row), flush=True)
    summary = [{"threads": t, "implementation": label,
                **{key: statistics.median(row[key] for row in rows if row["threads"] == t
                                         and row["implementation"] == label and not row["warmup"])
                   for key in ("update_seconds", "inference_seconds", "snapshot_seconds")}}
               for t in args.threads for label, _ in variants]
    write(args.output/"result.json", {"schema_version": 1, "config": config, "source": str(args.input or args.checkpoint),
                                     "source_sha256": hashlib.sha256((args.input or args.checkpoint).read_bytes()).hexdigest(),
                                     "device": args.device, "actor_device": "cpu", "tf32": False,
                                     "actor_threads": args.actor_threads,
                                     "deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
                                     "cudnn_deterministic": torch.backends.cudnn.deterministic,
                                     "cuda_runtime": torch.version.cuda,
                                     "gpu_name": torch.cuda.get_device_name() if args.device == "cuda" else None,
                                     "channels_last": args.channels_last,
                                     "convolution": args.convolution,
                                     "reference_update": args.reference_update, "reference_update_sha256": reference_hash,
                                     "provenance": provenance(catalog, args.config), "rows": rows, "summary": summary})
    print(json.dumps({"summary": summary}), flush=True)


if __name__ == "__main__":
    main()
