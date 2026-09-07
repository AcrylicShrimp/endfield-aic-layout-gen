"""Compare bounded CUDA rollout preparation on a repeated retained diagnostic input."""

import argparse
from copy import deepcopy
import hashlib
import json
import os
from pathlib import Path
import statistics
import time

import numpy as np
import torch

import policy
from reference.acceptance import Catalog
from evidence import provenance, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--copies", type=int, default=8)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    if args.output.exists() or min(args.copies, args.repeats) < 1:
        parser.error("output must be fresh; copies and repeats must be positive")
    if not torch.cuda.is_available():
        parser.error("CUDA preparation probe requested but unavailable")
    os.environ.setdefault("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.deterministic = True
    torch.backends.cudnn.benchmark = False
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    config = json.loads(args.config.read_text())
    torch.set_num_threads(config["threads"])
    torch.set_num_interop_threads(1)
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    saved = torch.load(args.input, map_location="cpu", weights_only=False)
    rollout = {key: list(value)*args.copies for key, value in saved["rollout"].items()}
    model = policy.Policy(rollout["observations"][0]).cuda()
    rows, parameter_hashes = [], set()
    for repeat in range(args.repeats+1):
        for budget in [1 << 30, 4 << 30][::1 if repeat % 2 == 0 else -1]:
            measured_config = {**config, "prepared_rollout_max_bytes": budget}
            model.load_state_dict(saved["policy"])
            optimizer = torch.optim.Adam(model.parameters(), lr=config["learning_rate"])
            optimizer.load_state_dict(deepcopy(saved["optimizer"]))
            torch.cuda.synchronize()
            torch.cuda.reset_peak_memory_stats()
            started = time.perf_counter()
            metrics = policy.update(model, optimizer, rollout, measured_config, np.random.default_rng(700017))
            torch.cuda.synchronize()
            seconds = time.perf_counter()-started
            digest = hashlib.sha256(b"".join(v.detach().cpu().numpy().tobytes()
                                            for v in model.state_dict().values())).hexdigest()
            parameter_hashes.add(digest)
            row = dict(budget_bytes=budget, repeat=repeat, warmup=repeat == 0, seconds=seconds,
                       peak_cuda_bytes=torch.cuda.max_memory_allocated(), metrics=metrics,
                       parameter_sha256=digest)
            rows.append(row)
            print(json.dumps(row), flush=True)
    summary = {str(b): statistics.median(r["seconds"] for r in rows if r["budget_bytes"] == b
                                         and not r["warmup"]) for b in (1 << 30, 4 << 30)}
    args.output.mkdir(parents=True)
    write(args.output/"result.json", dict(schema_version=1, provenance=provenance(catalog, args.config),
          input_sha256=hashlib.sha256(args.input.read_bytes()).hexdigest(), copies=args.copies,
          contract="Repeated diagnostic rollout only; identical updates under both memory ceilings",
          config=config, rows=rows, summary=summary, parameter_hashes_equal=len(parameter_hashes) == 1))
    if len(parameter_hashes) != 1:
        raise ValueError("preparation.parameter_mismatch: identical deterministic updates differed")


if __name__ == "__main__":
    main()
