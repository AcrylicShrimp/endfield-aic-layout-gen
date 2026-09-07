"""Compare complete Python/native replay, including conversion and optional CUDA inference."""

import argparse
import gc
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import time

import torch

from acceptance import Catalog
from environment import Environment
from environment_benchmark import replay, compare_rows
from native_observation import load_native, native_environment
from policy import Policy, single
from run import provenance, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--input", type=Path, nargs="+", required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--repeats", type=int, default=7)
    parser.add_argument("--cuda-inference", action="store_true")
    parser.add_argument("--collect-before-replay", action="store_true",
                        help="Collect prior trial garbage before setup; keep automatic GC enabled during replay")
    args = parser.parse_args()
    if args.output.exists() or args.repeats < 1:
        parser.error("output must be fresh and repeats positive")
    if args.cuda_inference and not torch.cuda.is_available():
        parser.error("CUDA inference requested but unavailable")
    root = Path(__file__).resolve().parents[2]
    config = json.loads(args.config.read_text())
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    catalog = Catalog(root, args.case.resolve())
    native = native_environment(load_native(args.native_library))
    rows = []
    for path in args.input:
        saved = torch.load(path, weights_only=False, map_location="cpu")
        actor = Policy(saved["rollout"]["observations"][0]).cuda() if args.cuda_inference else None
        if actor is not None:
            actor.load_state_dict(saved["policy"])
        for repeat in range(args.repeats+1):
            results = {}
            for label, implementation in [("python", Environment), ("native", native)][::1 if repeat % 2 == 0 else -1]:
                if args.collect_before_replay:
                    gc.collect()
                setup_seconds = 0.0
                inference_seconds = 0.0
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
                def factory(catalog, config):
                    nonlocal setup_seconds
                    started = time.perf_counter()
                    env = implementation(catalog, config)
                    setup_seconds = time.perf_counter()-started
                    if actor is not None:
                        observe = env.observe
                        generator = torch.Generator().manual_seed(700017)
                        def observing(candidates):
                            nonlocal inference_seconds
                            observation = observe(candidates)
                            started = time.perf_counter()
                            with torch.no_grad():
                                data = {key: value.cuda() for key, value in single(observation).items()}
                                logits, value = actor(data)
                                log_probs = logits.log_softmax(-1).cpu()
                                torch.multinomial(log_probs.exp(), 1, generator=generator).item()
                                value.cpu().item()
                            inference_seconds += time.perf_counter()-started
                            return observation
                        env.observe = observing
                    return env
                gc.callbacks.append(on_gc)
                try:
                    result = replay(factory, catalog, config, saved["rollout"])
                finally:
                    gc.callbacks.remove(on_gc)
                results[label] = result
                rows.append({"input": str(path), "implementation": label, "repeat": repeat,
                             "warmup": repeat == 0, "seconds": result[2], "setup_seconds": setup_seconds,
                             "inference_seconds": inference_seconds,
                             "gc_during_replay_call": collections,
                             "environment_seconds": result[3], "environment_counters": result[4]})
            compare_rows(results["native"], results["python"], saved["rollout"])
        print(json.dumps({"input": str(path), "parity": "bit_exact"}), flush=True)
    summary = [{"input": str(path), **{label: statistics.median(r["seconds"] for r in rows
                if r["input"] == str(path) and r["implementation"] == label and not r["warmup"])
                for label in ("python", "native")}} for path in args.input]
    sources = [root/"Cargo.toml", root/"Cargo.lock", *sorted((root/"crates/aic-pilot-native").rglob("*.rs")),
               root/"crates/aic-pilot-native/Cargo.toml"]
    args.output.mkdir(parents=True)
    write(args.output/"result.json", {"schema_version": 1, "provenance": provenance(catalog, args.config),
          "native_library_sha256": hashlib.sha256(args.native_library.read_bytes()).hexdigest(),
          "native_source_sha256": {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
          "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
          "build": "cargo build --release -p aic-pilot-native --features extension-module; PYO3_PYTHON set to experiment interpreter",
          "input_sha256": {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in args.input},
          "cuda_inference": args.cuda_inference, "trajectory": "Recorded actions; inferred actions discarded",
          "collect_before_replay": args.collect_before_replay,
          "gc_clock_scope": "Whole replay call, including setup and diagnostic serialization; not subtractable from timed replay",
          "parity": "bit_exact", "rows": rows, "summary": summary})
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
