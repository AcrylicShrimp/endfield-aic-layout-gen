"""Compare owned-native and Python-reference replay with the real per-action boundaries."""

import argparse
import gc
import hashlib
import json
from pathlib import Path
import statistics
import time

import torch

from evidence import provenance, write
from native import Runtime, layout, performance
from policy import Policy, single
from reference.acceptance import Catalog
from reference.environment import Environment, serialize


def replay(kind, runtime, catalog, config, saved, actor, inference_device):
    collections = []
    collection_started = None
    def on_gc(phase, info):
        nonlocal collection_started
        if phase == "start":
            collection_started = time.perf_counter()
        elif collection_started is not None:
            collections.append({"generation": info["generation"],
                                "seconds": time.perf_counter()-collection_started})
            collection_started = None
    gc.callbacks.append(on_gc)
    try:
        started = time.perf_counter()
        env = runtime.new_engine() if kind == "native" else Environment(catalog, config)
        setup_seconds = time.perf_counter()-started
        generator = torch.Generator().manual_seed(700017)
        rows = []
        inference_seconds = 0.0
        started = time.perf_counter()
        for selected in saved["rollout"]["actions"]:
            if kind == "native":
                observation, handle = env.observe()
            else:
                candidates = env.candidates()
                observation = env.observe(candidates)
            if actor is not None:
                inference_started = time.perf_counter()
                if inference_device == "cpu":
                    actor.act(observation, generator)
                else:
                    with torch.no_grad():
                        data = {key: value.cuda() for key, value in single(observation).items()}
                        logits, value = actor(data)
                        probabilities = logits.log_softmax(-1).cpu().exp()
                        torch.multinomial(probabilities, 1, generator=generator).item()
                        value.cpu().item()
                inference_seconds += time.perf_counter()-inference_started
            result = (env.step(handle, selected) if kind == "native"
                      else env.step(candidates.commands[selected].to_dict()))
            rows.append((observation, result))
            if result[1]:
                env.reset()
        elapsed = time.perf_counter()-started
        if kind == "native":
            states = [layout(env, field) for field in ("draft", "base", "best")]
            measured = performance(env)
        else:
            states = [serialize(getattr(env, field)) for field in ("draft", "base", "best")]
            measured = {"seconds": dict(env.seconds), "counters": dict(env.counters)}
        return {"observations_and_results": rows, "states": states, "seconds": elapsed,
                "setup_seconds": setup_seconds, "inference_seconds": inference_seconds,
                "environment_seconds": measured["seconds"], "environment_counters": measured["counters"],
                "gc_during_replay_call": collections}
    finally:
        gc.callbacks.remove(on_gc)


def compare(actual, expected, rollout):
    if actual["states"] != expected["states"]:
        raise ValueError("native.parity: final draft/base/best differ")
    for i, ((observation, result), (reference, reference_result)) in enumerate(zip(
            actual["observations_and_results"], expected["observations_and_results"])):
        if result != reference_result:
            raise ValueError(f"native.parity: transition differs at {i}")
        for key, value in observation.items():
            for wanted in (reference[key], rollout["observations"][i][key]):
                if value.dtype != wanted.dtype or value.shape != wanted.shape or value.tobytes() != wanted.tobytes():
                    raise ValueError(f"native.parity: observation differs at {i}: {key}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=11)
    parser.add_argument("--inference", choices=("none", "cpu", "cuda"), default="none")
    parser.add_argument("--collect-before-replay", action="store_true",
                        help="Collect prior trial garbage before setup; keep automatic GC enabled during replay")
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1:
        parser.error("output must be fresh and repeats positive")
    if args.inference == "cuda" and not torch.cuda.is_available():
        parser.error("CUDA inference requested but unavailable")
    config = json.loads(args.config.read_text())
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    runtime = Runtime(args.root, args.case, config, args.native_library)
    if runtime.build["profile"] != "release":
        parser.error("benchmark requires a release native library")
    catalog = Catalog(args.root, args.case)
    rows = []
    for path in args.input:
        saved = torch.load(path, weights_only=False, map_location="cpu")
        actor = None
        if args.inference != "none":
            actor = Policy(saved["rollout"]["observations"][0]).to(args.inference)
            actor.load_state_dict(saved["policy"])
        for repeat in range(args.repeats+1):
            results = {}
            for kind in ("python", "native")[::1 if repeat % 2 == 0 else -1]:
                if args.collect_before_replay:
                    gc.collect()
                result = replay(kind, runtime, catalog, config, saved, actor, args.inference)
                results[kind] = result
                rows.append({"input": str(path), "implementation": kind, "repeat": repeat, "warmup": repeat == 0,
                             **{key: value for key, value in result.items() if key not in ("observations_and_results", "states")}})
            compare(results["native"], results["python"], saved["rollout"])
        print(json.dumps({"input": str(path), "commands": len(saved["rollout"]["actions"]), "parity": "bit_exact"}), flush=True)
    summary = [{"input": str(path), **{kind: statistics.median(row["seconds"] for row in rows
                if row["input"] == str(path) and row["implementation"] == kind and not row["warmup"])
                for kind in ("python", "native")}} for path in args.input]
    args.output.mkdir(parents=True)
    write(args.output/"result.json", {"schema_version": 1, "provenance": provenance(runtime, args.config),
          "input_sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in args.input},
          "inference": args.inference, "trajectory": "Recorded actions; inferred actions discarded",
          "collect_before_replay": args.collect_before_replay,
          "setup_scope": "Native catalog load, engine construction and input identity verification; reference engine construction uses the already-loaded oracle catalog",
          "gc_clock_scope": "Whole replay call including setup and diagnostic exports; not subtractable from the timed loop",
          "parity": "bit_exact observations/transitions/final states; exhaustive domain parity is a separate diagnostic",
          "rows": rows, "summary": summary})
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
