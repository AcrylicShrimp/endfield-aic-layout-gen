"""Probe actor execution and two-episode batching on fixed retained observations."""

import argparse
from contextlib import nullcontext
import json
from pathlib import Path
import statistics
import time

import torch

from reference.acceptance import Catalog
from policy import Policy, batch, single
from evidence import provenance, write


def measure(model, observations, batch_size, device, inference):
    if len(observations) % batch_size:
        raise ValueError("Actor probe requires evenly sized episode groups")
    length = len(observations)//batch_size
    generator = torch.Generator().manual_seed(700017)
    seconds = dict(preparation=0.0, forward_and_transfer=0.0, sampling=0.0)
    values = []
    context = torch.inference_mode() if inference else torch.no_grad()
    started = time.perf_counter()
    with context:
        for t in range(length):
            group = [observations[t+episode*length] for episode in range(batch_size)]
            begin = time.perf_counter()
            data = single(group[0]) if batch_size == 1 else batch(group)
            data = {key: value.to(device) for key, value in data.items()}
            seconds["preparation"] += time.perf_counter()-begin
            begin = time.perf_counter()
            logits, value = model(data)
            # The real host environment needs sampled actions. Charge completed device work.
            log_probs = logits.log_softmax(-1).cpu()
            value = value.cpu()
            seconds["forward_and_transfer"] += time.perf_counter()-begin
            begin = time.perf_counter()
            for i, observation in enumerate(group):
                selected = torch.multinomial(log_probs[i].exp(), 1, generator=generator).item()
                values.append((selected, log_probs[i, selected].item(), value[i].item()))
            seconds["sampling"] += time.perf_counter()-begin
    return {"seconds": time.perf_counter()-started, "parts": seconds,
            "samples": len(values), "finite": all(torch.isfinite(torch.tensor(row[1:])).all() for row in values)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, type=Path)
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--threads", type=int, default=16)
    parser.add_argument("--repeats", type=int, default=3)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f"Refusing to overwrite {args.output}")
    args.output.mkdir(parents=True)
    torch.set_num_threads(args.threads)
    torch.set_num_interop_threads(1)
    torch.backends.cuda.matmul.allow_tf32 = False
    torch.backends.cudnn.allow_tf32 = False
    torch.backends.cudnn.benchmark = False
    catalog = Catalog(Path(__file__).resolve().parents[2], args.case.resolve())
    saved = torch.load(args.input, map_location="cpu", weights_only=False)
    observations = saved["rollout"]["observations"]
    rows = []
    for device in ("cpu", "cuda"):
        model = Policy(observations[0]).to(device)
        model.load_state_dict(saved["policy"])
        variants = [(batch_size, inference) for batch_size in (1, 2) for inference in (False, True)]
        for repeat in range(args.repeats+1):
            for batch_size, inference in variants[::1 if repeat % 2 == 0 else -1]:
                result = measure(model, observations, batch_size, device, inference)
                rows.append({"device": device, "batch_size": batch_size, "inference_mode": inference,
                             "repeat": repeat, "warmup": repeat == 0, **result})
                print(json.dumps(rows[-1]), flush=True)
    summary = [{"device": device, "batch_size": size, "inference_mode": inference,
                "seconds": statistics.median(r["seconds"] for r in rows if r["device"] == device
                                              and r["batch_size"] == size and r["inference_mode"] == inference and not r["warmup"])}
               for device in ("cpu", "cuda") for size, inference in variants]
    write(args.output/"result.json", {"schema_version": 1, "input": str(args.input), "threads": args.threads,
                                     "provenance": provenance(catalog, args.config), "rows": rows, "summary": summary})
    print(json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()
