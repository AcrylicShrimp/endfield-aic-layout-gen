"""CPU thread costs on real root observations; not a quality or GPU experiment."""

import argparse
import json
from pathlib import Path
import resource
import statistics
import time

import torch

from client import Worker
from policy import Policy, pack


def measure(function, repeats):
    for _ in range(5):
        function()
    values = []
    for _ in range(repeats):
        start = time.perf_counter()
        function()
        values.append((time.perf_counter() - start) * 1000)
    values.sort()
    return dict(median_ms=statistics.median(values), p95_ms=values[min(len(values) - 1, int(.95 * len(values)))], repeats=repeats)


def run(options):
    torch.set_num_threads(options.threads)
    torch.set_num_interop_threads(1)
    torch.manual_seed(7)
    output = Path(options.output_directory).resolve()
    output.mkdir()
    worker = Worker(Path(options.roots).resolve(), output / "worker")
    rows = []
    try:
        roots = worker.call("hello")["roots"]
        selected = [next(r for r in roots if r["split"] == "train"), next(r for r in roots if r["generation"] == "closure")]
        observations = [worker.call("reset", root=r["id"]) for r in selected]
    finally:
        worker.close()
    for observation in observations:
        model = Policy(len(observation["features"]["tokens"][0]), len(observation["features"]["actions"][0]), width=32)
        optimizer = torch.optim.Adam(model.parameters(), lr=.001)
        cases = []
        for batch in (1, 8):
            obs = [observation] * batch
            inputs, mask = pack(obs)
            def forward():
                with torch.no_grad():
                    model(*inputs)
            def update():
                logits = model(*inputs).masked_fill(~mask, -torch.inf)
                loss = -torch.distributions.Categorical(logits=logits).entropy().mean()
                optimizer.zero_grad(set_to_none=True)
                loss.backward()
                optimizer.step()
            cases.append(dict(batch=batch, encode=measure(lambda: pack(obs), options.repeats),
                              forward=measure(forward, options.repeats), update=measure(update, options.repeats)))
        rows.append(dict(root=observation["root"], tokens=len(observation["features"]["tokens"]),
                         actions=len(observation["features"]["actions"]), parameters=sum(p.numel() for p in model.parameters()), cases=cases))
    with output.joinpath("report.json").open("x") as stream:
        json.dump(dict(schema_version=1, device="cpu", threads=options.threads, torch=torch.__version__, rows=rows,
                       process_peak_rss_bytes_macos=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
                       scope="Real full-factory features; arithmetic timing, not learned quality; entropy update is a cost probe"), stream)
    print(json.dumps(dict(status="complete", threads=options.threads, output=str(output))), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--roots", required=True)
    parser.add_argument("--output-directory", required=True)
    parser.add_argument("--threads", type=int, required=True, choices=[1, 2, 4, 8])
    parser.add_argument("--repeats", type=int, default=30)
    try:
        run(parser.parse_args())
    except Exception as error:
        print(json.dumps(dict(code="rl-cost-failed", severity="error", stage="cpu-cost", message=str(error), entity=None)))
        raise SystemExit(1)
