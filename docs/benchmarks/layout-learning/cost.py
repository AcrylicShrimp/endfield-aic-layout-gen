"""CPU tensor-cost stress test, explicitly not a layout-quality experiment."""

import argparse
import json
import math
import resource
import sys
import time
from pathlib import Path

import torch
from model import Policy
from train import read, write, sha


def measure(fn, repeats):
    for _ in range(3):
        fn()
    times = []
    for _ in range(repeats):
        started = time.perf_counter()
        fn()
        times.append(1000 * (time.perf_counter() - started))
    times.sort()
    return {"p50_ms": times[len(times) // 2], "p95_ms": times[math.ceil(len(times) * .95) - 1], "repeats": repeats}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--report-output", type=Path, required=True)
    options = parser.parse_args()
    config = read(options.config)
    if config["schema_version"] != 1 or not 1 <= config["repeats"] <= 100:
        raise ValueError("Invalid cost plan")
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.manual_seed(config["seed"])
    results = []
    for case in config["cases"]:
        n, width, batch = case["tokens"], case["width"], case["batch"]
        if not 1 <= n <= 2048 or not 1 <= width <= 128 or width % 2 or not 1 <= batch <= 16 or batch * n * n > 16 * 1024 * 1024:
            raise ValueError("Cost-only tensor guard exceeded")
        model = Policy(14, 21, width, "graph")
        adjacency = torch.zeros(batch, n, n)
        indices = torch.arange(n - 1)
        adjacency[:, indices, indices + 1] = 1
        inputs = (torch.randn(batch, n, 14), adjacency, torch.randn(batch, 12, 21), torch.ones(batch, n, dtype=torch.bool))
        model.eval()
        with torch.no_grad():
            single = measure(lambda: model(*(t[:1] for t in inputs)), config["repeats"])
            forward = measure(lambda: model(*inputs), config["repeats"])
        optimizer = torch.optim.Adam(model.parameters(), lr=.001)
        model.train()
        def step():
            optimizer.zero_grad(set_to_none=True)
            loss = -model(*inputs).log_softmax(-1)[:, 0].mean()
            loss.backward()
            optimizer.step()
        backward = measure(step, config["repeats"])
        results.append({**case, "parameters": sum(p.numel() for p in model.parameters()), "single_forward": single,
                        "batch_forward": forward, "update": backward, "input_tensor_bytes": sum(t.numel() * t.element_size() for t in inputs),
                        "process_cumulative_peak_rss_bytes_macos": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss})
    write(options.report_output, {"schema_version": 1, "device": "cpu", "threads": 1, "torch": torch.__version__,
                                 "scope": "random tensors, dense adjacency; NOT validated layouts, factory training or a quality prediction",
                                 "config_sha256": sha(options.config), "results": results})
    print(json.dumps(results))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(json.dumps({"code": "learning-cost-failed", "severity": "error", "stage": "cost-only",
                          "message": str(error), "entity": sys.argv[1:]}), file=sys.stderr)
        sys.exit(1)
