"""Separate final lower- and outer-model effects on the fixed diagnostic."""

import argparse
from pathlib import Path
import statistics

from hierarchical_horizon_onpolicy import require
from hierarchical_lower_restored import (checkpoint, mixed_outer, search,
                                         search_config, validate as validate_parent)
from hierarchical_target_learner import BINARY, STREAMS, outcomes
from ppo_harness_state import read, sha, write


NEW_CELLS = (("h6", "h12"), ("h12", "h6"))


def diagnostic_trigger(parent, primary):
    if not primary["passes_primary_gate"]:
        return "failed-primary", None
    heldout = parent / "heldout-check"
    protocol = read(heldout / "protocol.json")
    progress = read(heldout / "progress.json")
    summary = read(heldout / "summary.json")
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"] == 21
            and summary["protocol_sha256"] == sha(heldout / "protocol.json")
            and summary["progress_sha256"] == sha(heldout / "progress.json")
            and summary["passes_heldout_gate"],
            "passed held-out closeout required")
    return "passed-heldout", sha(heldout / "summary.json")


def pair_models(parent, stream, lower_arm, outer_arm):
    lower, _ = checkpoint(parent, stream, 2, lower_arm)
    if outer_arm == "h6":
        outer = mixed_outer(parent, stream, 2)
    else:
        _, outer = checkpoint(parent, stream, 2, "h12")
    require(read(outer / "config.json")["search"]
            == read(search_config(parent, "h12")),
            f"outer search contract changed: {stream}-{outer_arm}")
    return lower, outer


def cell_name(stream, lower_arm, outer_arm):
    return f"{stream}-lower-{lower_arm}-outer-{outer_arm}"


def prepare(parent):
    parent_protocol, parent_progress = validate_parent(parent)
    primary = read(parent / "primary-summary.json")
    observer = read(parent / "wandb-observer.json")
    require(parent_progress["status"] == "complete"
            and len(parent_progress["records"]) == parent_protocol["planned_records"]
            and primary["progress_sha256"] == sha(parent / "progress.json")
            and observer["status"] == "finished"
            and observer["exported_records"] == parent_protocol["planned_records"],
            "parent and tracking closeout required")
    trigger, heldout_summary_sha256 = diagnostic_trigger(parent, primary)
    cross = parent / "lower-outer-cross"
    require(not cross.exists(), "cross output already exists")
    models = {}
    for stream in STREAMS:
        for lower_arm in ("h6", "h12"):
            for outer_arm in ("h6", "h12"):
                lower, outer = pair_models(parent, stream, lower_arm, outer_arm)
                name = cell_name(stream, lower_arm, outer_arm)
                models[name] = {
                    "lower_sha256": sha(lower / "state.safetensors"),
                    "outer_sha256": sha(outer / "state.safetensors"),
                    "outer_config_sha256": sha(outer / "config.json"),
                }
                if lower_arm == outer_arm:
                    existing_name = f"{stream}-g2-"
                    existing_name += "h12" if lower_arm == "h12" else "mixed"
                    existing = next(row for row in parent_progress["records"]
                                    if row["name"] == existing_name)
                    require(existing["lower_checkpoint_sha256"] == models[name][
                        "lower_sha256"] and existing["outer_checkpoint_sha256"]
                        == models[name]["outer_sha256"],
                        f"existing same-origin cell changed: {name}")
    cross.mkdir()
    write(cross / "protocol.json", {
        "schema": "aic-hierarchical-lower-outer-cross-protocol-v1",
        "runner_sha256": sha(Path(__file__)),
        "parent_protocol_sha256": sha(parent / "protocol.json"),
        "parent_progress_sha256": sha(parent / "progress.json"),
        "primary_summary_sha256": sha(parent / "primary-summary.json"),
        "trigger": trigger,
        "heldout_summary_sha256": heldout_summary_sha256,
        "binary_sha256": sha(BINARY),
        "panel_sha256": sha(parent / "panels/lower-diagnostic.json"),
        "search_sha256": sha(search_config(parent, "h12")),
        "models": models,
        "planned_records": len(STREAMS) * len(NEW_CELLS),
    })
    write(cross / "progress.json", {
        "schema": "aic-hierarchical-lower-outer-cross-progress-v1",
        "protocol_sha256": sha(cross / "protocol.json"),
        "status": "prepared", "records": [],
    })
    print("prepared=3 streams x 2 missing lower/outer combinations")


def validate(parent):
    parent_protocol, parent_progress = validate_parent(parent)
    cross = parent / "lower-outer-cross"
    protocol = read(cross / "protocol.json")
    progress = read(cross / "progress.json")
    trigger, heldout_summary_sha256 = diagnostic_trigger(
        parent, read(parent / "primary-summary.json"))
    require(parent_progress["status"] == "complete"
            and progress["protocol_sha256"] == sha(cross / "protocol.json")
            and protocol["runner_sha256"] == sha(Path(__file__))
            and protocol["parent_protocol_sha256"] == sha(parent / "protocol.json")
            and protocol["parent_progress_sha256"] == sha(parent / "progress.json")
            and protocol["primary_summary_sha256"] == sha(parent / "primary-summary.json")
            and protocol["trigger"] == trigger
            and protocol["heldout_summary_sha256"] == heldout_summary_sha256
            and protocol["binary_sha256"] == sha(BINARY)
            and protocol["panel_sha256"] == sha(parent / "panels/lower-diagnostic.json")
            and protocol["search_sha256"] == sha(search_config(parent, "h12"))
            and protocol["planned_records"] == len(STREAMS) * len(NEW_CELLS),
            "cross input changed")
    for stream in STREAMS:
        for lower_arm in ("h6", "h12"):
            for outer_arm in ("h6", "h12"):
                name = cell_name(stream, lower_arm, outer_arm)
                lower, outer = pair_models(parent, stream, lower_arm, outer_arm)
                require(protocol["models"][name] == {
                    "lower_sha256": sha(lower / "state.safetensors"),
                    "outer_sha256": sha(outer / "state.safetensors"),
                    "outer_config_sha256": sha(outer / "config.json"),
                }, f"cross model changed: {name}")
    return protocol, progress, parent_progress


def run(parent, max_records):
    protocol, progress, _ = validate(parent)
    cross = parent / "lower-outer-cross"
    panel = parent / "panels/lower-diagnostic.json"
    cases = read(panel)["cases"]
    order = [case["id"] for case in cases]
    easy = [len(case["problem"]["connections"]) <= 4 for case in cases]
    progress["status"] = "running"
    write(cross / "progress.json", progress)
    added = 0
    for stream in STREAMS:
        for lower_arm, outer_arm in NEW_CELLS:
            name = cell_name(stream, lower_arm, outer_arm)
            if any(row["name"] == name for row in progress["records"]):
                continue
            if max_records is not None and added >= max_records:
                return
            lower, outer = pair_models(parent, stream, lower_arm, outer_arm)
            output = cross / "evaluations" / f"{name}.json"
            report, seconds = search(cross, name, panel, lower, outer,
                                     search_config(parent, "h12"), output)
            progress["records"].append(dict(outcomes(report, order, easy),
                                            name=name, stream=stream,
                                            lower_arm=lower_arm, outer_arm=outer_arm,
                                            report_sha256=sha(output),
                                            evaluation_seconds=seconds))
            write(cross / "progress.json", progress)
            added += 1
            print(f"cross={name} completed={progress['records'][-1]['completed']}/64",
                  flush=True)
    require(len(progress["records"]) == protocol["planned_records"],
            "cross evaluations incomplete")
    progress["status"] = "complete"
    write(cross / "progress.json", progress)


def summarize(parent):
    protocol, progress, parent_progress = validate(parent)
    require(progress["status"] == "complete"
            and len(progress["records"]) == protocol["planned_records"],
            "cross evaluations incomplete")
    cross = parent / "lower-outer-cross"
    base_mask = parent_progress["baseline"]["case_success"]
    require(len(base_mask) == 64, "starting diagnostic mask changed")
    new_rows = {row["name"]: row for row in progress["records"]}
    old_rows = {row["name"]: row for row in parent_progress["records"]}
    streams = []
    for stream in STREAMS:
        cells = {}
        for lower_arm in ("h6", "h12"):
            for outer_arm in ("h6", "h12"):
                name = cell_name(stream, lower_arm, outer_arm)
                record = (old_rows[f"{stream}-g2-"
                                   f"{'mixed' if lower_arm == 'h6' else 'h12'}"]
                          if lower_arm == outer_arm else new_rows[name])
                if lower_arm != outer_arm:
                    require(record["report_sha256"]
                            == sha(cross / "evaluations" / f"{name}.json"),
                            f"cross report changed: {name}")
                mask = record["case_success"]
                require(len(mask) == 64, f"cross mask changed: {name}")
                cells[f"lower-{lower_arm}-outer-{outer_arm}"] = {
                    "completed": record["completed"],
                    "case_success": mask,
                    "evaluation_seconds": record["evaluation_seconds"],
                    "lost_from_start": sum(a == "0" and b == "1"
                                           for a, b in zip(mask, base_mask)),
                }
        completion = {key: value["completed"] for key, value in cells.items()}
        ll = completion["lower-h6-outer-h6"]
        lh = completion["lower-h6-outer-h12"]
        hl = completion["lower-h12-outer-h6"]
        hh = completion["lower-h12-outer-h12"]
        streams.append({
            "stream": stream, "cells": cells,
            "lower_effect_with_h6_outer": hl - ll,
            "lower_effect_with_h12_outer": hh - lh,
            "outer_effect_with_h6_lower": lh - ll,
            "outer_effect_with_h12_lower": hh - hl,
            "interaction": hh - hl - lh + ll,
        })
    summary = {
        "schema": "aic-hierarchical-lower-outer-cross-summary-v1",
        "protocol_sha256": sha(cross / "protocol.json"),
        "progress_sha256": sha(cross / "progress.json"),
        "streams": streams,
        "mean_effects": {name: statistics.mean(row[name] for row in streams)
                         for name in ("lower_effect_with_h6_outer",
                                      "lower_effect_with_h12_outer",
                                      "outer_effect_with_h6_lower",
                                      "outer_effect_with_h12_lower", "interaction")},
        "interpretation_limit": "mechanism on reused diagnostic; no promotion decision",
    }
    write(cross / "summary.json", summary)
    print("mean effects:", summary["mean_effects"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "run", "summarize"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-records", type=int)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.output)
    elif args.command == "run":
        run(args.output, args.max_records)
    else:
        summarize(args.output)
