"""Compare the retained H12 models with a search depth cap that cannot bind."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import time


STREAMS = ("s1", "s2", "s3")
SEEDS = (20260922, 20260923, 20260924)
INERT_HORIZON = 4096  # Each root search has only 128 one-node-growth simulations.
SOURCE_FILES = (
    "configs/search-h12.json",
    "heldout-check/progress.json",
    "heldout-check/protocol.json",
    "panels/heldout.json",
)


def read(path):
    return json.loads(path.read_text())


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-uncapped-search: {message}")


def label(stream, seed):
    return f"{stream}-{seed}"


def prepare(root, archive, binary):
    require(not root.exists(), f"output already exists: {root}")
    root.mkdir(parents=True)
    source = root / "source"
    with tarfile.open(archive, "r:gz") as bundle:
        names = list(SOURCE_FILES)
        for stream in STREAMS:
            base = f"training/{stream}-g2-h12"
            names.extend((f"{base}/{model}/{filename}"
                          for model in ("lower", "outer")
                          for filename in ("config.json", "parameter-order.json",
                                           "state.safetensors")))
        for name in names:
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            with bundle.extractfile(name) as input_file, target.open("wb") as output:
                shutil.copyfileobj(input_file, output)
    prior = read(source / "heldout-check/protocol.json")
    require(sha(binary) == prior["binary_sha256"], "binary differs from retained evaluation")
    require(sha(source / "panels/heldout.json") == prior["panel_sha256"],
            "held-out panel differs from retained evaluation")
    original = read(source / "configs/search-h12.json")
    require(original["horizon"] == 12 and original["simulations_per_move"] == 128,
            "expected H12/128 source config")
    # A simulation grows the tree by at most one node, so 128 simulations
    # cannot reach 4096 actions from the root.
    require(INERT_HORIZON > original["simulations_per_move"],
            "the depth cap may still bind")
    config_hashes = {}
    model_hashes = {}
    for seed in SEEDS:
        config = json.loads(json.dumps(original))
        config["horizon"] = INERT_HORIZON
        config["root_selection"]["seed"] = seed
        config_path = root / "configs" / f"search-{seed}.json"
        write(config_path, config)
        config_hashes[str(seed)] = sha(config_path)
        for stream in STREAMS:
            base = source / "training" / f"{stream}-g2-h12"
            shadow = root / "models" / label(stream, seed)
            shadow.mkdir(parents=True)
            for filename in ("state.safetensors", "parameter-order.json"):
                shutil.copy2(base / "outer" / filename, shadow / filename)
            model_config = read(base / "outer/config.json")
            require(model_config["search"] == original,
                    f"unexpected outer model contract: {stream}")
            model_config["search"] = config
            write(shadow / "config.json", model_config)
            model_hashes[label(stream, seed)] = {
                "lower": sha(base / "lower/state.safetensors"),
                "outer": sha(shadow / "state.safetensors"),
                "shadow_config": sha(shadow / "config.json"),
            }
    smoke = read(source / "panels/heldout.json")
    smoke["cases"] = smoke["cases"][:8]
    write(root / "panels/smoke.json", smoke)
    write(root / "protocol.json", {
        "schema": "aic-hierarchical-uncapped-search-protocol-v1",
        "archive_sha256": sha(archive),
        "binary_sha256": sha(binary),
        "source_panel_sha256": sha(source / "panels/heldout.json"),
        "source_baseline_sha256": sha(source / "heldout-check/progress.json"),
        "source_search_sha256": sha(source / "configs/search-h12.json"),
        "config_sha256": config_hashes,
        "model_sha256": model_hashes,
        "inert_horizon": INERT_HORIZON,
        "planned_full_records": len(STREAMS) * len(SEEDS),
    })
    write(root / "progress.json", {"status": "prepared", "records": []})
    print(f"prepared={root} smoke_cases=8 full_cases=128")


def run(root, binary, smoke, max_records):
    source = root / "source"
    protocol = read(root / "protocol.json")
    require(sha(binary) == protocol["binary_sha256"], "binary changed")
    require(sha(source / "panels/heldout.json") == protocol["source_panel_sha256"],
            "panel changed")
    progress_path = root / ("smoke-progress.json" if smoke else "progress.json")
    progress = read(progress_path) if progress_path.exists() else {"records": []}
    panel = root / "panels/smoke.json" if smoke else source / "panels/heldout.json"
    progress["status"] = "running"
    write(progress_path, progress)
    added = 0
    for seed in SEEDS:
        for stream in STREAMS:
            name = label(stream, seed)
            if any(record["name"] == name for record in progress["records"]):
                continue
            if max_records is not None and added >= max_records:
                return
            model = source / "training" / f"{stream}-g2-h12/lower"
            shadow = root / "models" / name
            config = root / "configs" / f"search-{seed}.json"
            hashes = protocol["model_sha256"][name]
            require(sha(config) == protocol["config_sha256"][str(seed)]
                    and sha(model / "state.safetensors") == hashes["lower"]
                    and sha(shadow / "state.safetensors") == hashes["outer"]
                    and sha(shadow / "config.json") == hashes["shadow_config"],
                    f"inputs changed: {name}")
            output = root / ("smoke" if smoke else "raw") / f"{name}.json"
            output.parent.mkdir(parents=True, exist_ok=True)
            log = root / "logs" / f"{name}-{'smoke' if smoke else 'full'}.log"
            log.parent.mkdir(parents=True, exist_ok=True)
            command = [str(binary), "learning", "hierarchical-search-harness",
                       "--config", str(config), "--panel", str(panel),
                       "--lower-training-output", str(model),
                       "--outer-training-output", str(shadow),
                       "--catalog-root", "data/game/normalized",
                       "--output", str(output), "--device", "cuda:0"]
            started = time.monotonic()
            with log.open("w") as handle:
                result = subprocess.run(command, stdout=handle,
                                        stderr=subprocess.STDOUT, check=False)
            require(result.returncode == 0, f"search failed: {name}; see {log}")
            report = read(output)
            require(report["status"] == "complete"
                    and report["lower_checkpoint_sha256"] == hashes["lower"]
                    and report["outer_checkpoint_sha256"] == hashes["outer"]
                    and len(report["cases"]) == (8 if smoke else 128),
                    f"search lineage differs: {name}")
            mask = "".join("1" if case["mcts"]["fully_validated"] else "0"
                           for case in report["cases"])
            progress["records"].append({
                "name": name, "seed": seed, "stream": stream,
                "completed": mask.count("1"), "case_success": mask,
                "wall_seconds": time.monotonic() - started,
                "report_sha256": sha(output),
            })
            write(progress_path, progress)
            added += 1
            print(f"{'smoke' if smoke else 'full'}={name} completed={mask.count('1')}/{len(mask)}"
                  f" seconds={progress['records'][-1]['wall_seconds']:.1f}", flush=True)
    progress["status"] = "complete"
    write(progress_path, progress)


def summarize(root):
    progress = read(root / "progress.json")
    require(len(progress["records"]) == len(SEEDS) * len(STREAMS),
            "full comparison incomplete")
    baseline = read(root / "source/heldout-check/progress.json")
    old = {(r["stream"], r["seed"]): r for r in baseline["records"]
           if r["arm"] == "h12"}
    rows = []
    for record in progress["records"]:
        control = old[(record["stream"], record["seed"])]
        a, b = control["case_success"], record["case_success"]
        require(len(a) == len(b) == 128, "paired masks differ")
        rows.append({
            "stream": record["stream"], "seed": record["seed"],
            "h12_completed": control["completed"],
            "uncapped_completed": record["completed"],
            "completion_gain": record["completed"] - control["completed"],
            "gained": sum(x == "0" and y == "1" for x, y in zip(a, b)),
            "lost": sum(x == "1" and y == "0" for x, y in zip(a, b)),
            "h12_seconds": control["evaluation_seconds"],
            "uncapped_seconds": record["wall_seconds"],
        })
    write(root / "summary.json", {
        "schema": "aic-hierarchical-uncapped-search-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "rows": rows,
        "mean_completion_gain": sum(r["completion_gain"] for r in rows) / len(rows),
        "winning_pairs": sum(r["completion_gain"] > 0 for r in rows),
        "mean_runtime_ratio": sum(r["uncapped_seconds"] / r["h12_seconds"]
                                  for r in rows) / len(rows),
    })
    print(json.dumps(read(root / "summary.json"), indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize"))
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--binary", type=Path, default=Path("target/release/aic-cli"))
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        require(args.archive is not None, "--archive required for prepare")
        prepare(args.root, args.archive, args.binary)
    elif args.command == "run":
        run(args.root, args.binary, args.smoke, args.max_records)
    else:
        summarize(args.root)
