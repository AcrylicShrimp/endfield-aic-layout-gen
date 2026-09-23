"""Close out the fixed 2x2 hierarchical policy-stability screen compactly."""

import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import statistics

import numpy as np
from safetensors.numpy import load_file


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()


def read(path):
    return json.loads(path.read_text())


def require(condition, message):
    if not condition:
        raise ValueError(message)


def model_distance(first, second, prefix=None):
    keys = [key for key in first if prefix is None or key.startswith(prefix)]
    require(set(first) == set(second) and keys, "model parameters differ")
    numerator = denominator = 0.0
    for key in keys:
        a = first[key].astype(np.float64)
        b = second[key].astype(np.float64)
        numerator += np.square(a - b).sum()
        denominator += np.square(a).sum()
    return float(math.sqrt(numerator / max(denominator, 1e-30)))


def root_diagnostics(collection):
    buckets = {"successful": [], "failed": []}
    terminations = Counter()
    for case in collection["cases"]:
        terminations[case["mcts"]["termination"]] += 1
        for root in case["mcts"]["root_records"]:
            actions = root["actions"]
            require(actions and root["selected_action"] < len(actions), "invalid root")
            priors = [max(0.0, action["prior"]) for action in actions]
            total = sum(priors)
            require(total > 0, "zero root prior")
            normalized = [p / total for p in priors]
            entropy = -sum(p * math.log(p) for p in normalized if p > 0)
            if len(actions) > 1:
                entropy /= math.log(len(actions))
            bucket = "successful" if root["later_fully_validated"] else "failed"
            buckets[bucket].append({
                "entropy": entropy,
                "max_prior": max(normalized),
                "unvisited_target_mass": sum(
                    action["policy_target"] for action in actions if action["visits"] == 0
                ),
            })
    result = {"termination_counts": dict(sorted(terminations.items()))}
    for name, roots in buckets.items():
        result[name] = {"count": len(roots)}
        for metric in ("entropy", "max_prior", "unvisited_target_mass"):
            result[name][f"mean_{metric}"] = (
                statistics.mean(root[metric] for root in roots) if roots else None
            )
    return result


def evaluation(report, baseline, strata, prior):
    require(report["partition"] == "training" and not report["root_records_retained"],
            "diagnostic evaluation contract changed")
    success = {case["id"]: case["mcts"]["fully_validated"] for case in report["cases"]}
    require(set(success) == set(baseline), "diagnostic identities changed")
    def contrast(reference):
        return {
            "gained": sum(success[k] and not reference[k] for k in success),
            "lost": sum(reference[k] and not success[k] for k in success),
        }
    connection = {}
    for count in sorted(set(strata.values())):
        ids = [key for key, value in strata.items() if value == count]
        connection[str(count)] = {"completed": sum(success[key] for key in ids), "cases": len(ids)}
    return {
        "completed": sum(success.values()),
        "cases": len(success),
        "vs_start": contrast(baseline),
        "vs_prior_evaluation": contrast(prior) if prior is not None else None,
        "connection_strata": connection,
        "termination_counts": dict(sorted(Counter(
            case["mcts"]["termination"] for case in report["cases"]
        ).items())),
        "case_success": success,
    }


def bytes_in_tree(path):
    return sum(item.stat().st_size for item in path.rglob("*") if item.is_file())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--screen", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    screen, inputs = args.screen, args.inputs
    baseline_record = read(inputs / "baseline-evidence.json")
    baseline = baseline_record["case_success"]
    diagnostic_panel = read(inputs / "diagnostic-panel.json")
    case_order = [case["id"] for case in diagnostic_panel["cases"]]
    strata = {case["id"]: len(case["problem"]["connections"])
              for case in diagnostic_panel["cases"]}
    require(set(strata) == set(baseline), "baseline panel identities changed")
    original = load_file(inputs / "initial-outer/state.safetensors")
    lower_sha = digest(inputs / "initial-lower/state.safetensors")
    output = {
        "schema": "aic-hierarchical-stability-screen-evidence-v1",
        "source_commit": "31555f5",
        "diagnostic_panel_sha256": digest(inputs / "diagnostic-panel.json"),
        "baseline_evidence_sha256": digest(inputs / "baseline-evidence.json"),
        "baseline_completed": sum(baseline.values()),
        "diagnostic_case_ids": case_order,
        "arms": {},
    }
    for arm in "abcd":
        root = screen / f"arm-{arm}"
        state = read(root / "state.json")
        observer = read(root / "wandb-observer.json")
        require(state["completed_generations"] == 8, f"arm {arm} incomplete")
        require(observer["status"] == "finished" and observer["remote_confirmed_step"] >= 7,
                f"arm {arm} W&B unacknowledged")
        prior_evaluation = None
        prior_model = original
        generations = []
        case_masks = {case_id: 0 for case_id in case_order}
        prior_time = (root / "config.json").stat().st_mtime
        for generation in range(8):
            directory = root / "generations" / f"{generation:06}"
            panel = directory / "training-panel.json"
            common = inputs / "common-panels" / f"{generation:06}.json"
            require(digest(panel) == digest(common), f"{arm} generation {generation} panel differs")
            training = read(directory / "training/report.json")
            summary = read(directory / "summary.json")
            require(training["updates"] == 3200 and training["outer_states_per_epoch"] == 256,
                    f"{arm} generation {generation} update budget differs")
            require(training["lower_checkpoint_sha256"] == lower_sha and not training["train_lower"],
                    f"{arm} generation {generation} lower model changed")
            require(training["train_failed_policy"] == (arm in "ab"),
                    f"{arm} generation {generation} failed target setting differs")
            if generation == 1:
                require((training["replay_root_states"] > 0) == (arm in "bd"),
                        f"{arm} replay treatment differs")
            collection_path = directory / "collection.json"
            roots = root_diagnostics(read(collection_path))
            require(
                roots["successful"]["count"] + roots["failed"]["count"]
                == training["root_states"],
                f"{arm} generation {generation} root count differs",
            )
            model_path = directory / "training/outer/state.safetensors"
            model = load_file(model_path)
            finished = (directory / "summary.json").stat().st_mtime
            row = {
                "generation": generation,
                "panel_sha256": digest(panel),
                "collection_sha256": digest(collection_path),
                "training_report_sha256": digest(directory / "training/report.json"),
                "lower_checkpoint_sha256": training["lower_checkpoint_sha256"],
                "outer_checkpoint_sha256": training["outer_checkpoint_sha256"],
                "training_completed": summary["training_completed"],
                "training_cases": summary["training_cases"],
                "root_states": training["root_states"],
                "replay_root_states": training["replay_root_states"],
                "replay_slots_per_epoch": 128 if arm in "bd" and training["replay_root_states"] else 0,
                "successful_policy_roots": roots["successful"]["count"],
                "failed_policy_roots": roots["failed"]["count"] if arm in "ab" else 0,
                "updates": training["updates"],
                "training_seconds": training["elapsed_seconds"],
                "integrated_generation_wall_seconds": finished - prior_time,
                "outer_relative_l2_from_start": model_distance(original, model),
                "outer_relative_l2_from_prior": model_distance(prior_model, model),
                "policy_head_relative_l2_from_start": model_distance(
                    original, model, "outer.model.policy."
                ),
                "roots": roots,
            }
            if generation % 2 == 1:
                evaluation_path = directory / "heldout.json"
                report = read(evaluation_path)
                row["evaluation_report_sha256"] = digest(evaluation_path)
                row["evaluation"] = evaluation(report, baseline, strata, prior_evaluation)
                for case_id, success in row["evaluation"]["case_success"].items():
                    case_masks[case_id] |= int(success) << (generation // 2)
                row["evaluation"].update({
                    "mean_footprint_area": summary["heldout_mean_footprint_area"],
                    "mean_transport_cells": summary["heldout_mean_transport_cells"],
                    "mean_turns": summary["heldout_mean_turns"],
                })
                prior_evaluation = row["evaluation"]["case_success"]
                del row["evaluation"]["case_success"]
            generations.append(row)
            prior_model = model
            prior_time = finished
        tail = [generations[g]["evaluation"]["completed"] for g in (5, 7)]
        output["arms"][arm] = {
            "wandb_url": observer["url"],
            "wandb_remote_confirmed_step": observer["remote_confirmed_step"],
            "output_report_sha256": digest(root / "report.json"),
            "total_integrated_wall_seconds": (root / "report.json").stat().st_mtime
            - (root / "config.json").stat().st_mtime,
            "raw_output_bytes": bytes_in_tree(root),
            "last_two_evaluations": tail,
            "last_two_mean": statistics.mean(tail),
            "last_two_minimum": min(tail),
            "evaluation_success_masks_hex": "".join(
                format(case_masks[case_id], "x") for case_id in case_order
            ),
            "generations": generations,
        }
        print(arm, tail, "minimum", min(tail), flush=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    print(f"evidence_bytes={args.output.stat().st_size}", flush=True)


if __name__ == "__main__":
    main()
