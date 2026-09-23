"""Close out one stream of the predeclared policy-stability confirmation."""

import argparse
import json
from pathlib import Path
import statistics

from safetensors.numpy import load_file

from analyze_hierarchical_stability_screen import (
    bytes_in_tree,
    digest,
    evaluation,
    model_distance,
    read,
    require,
    root_diagnostics,
)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--confirmation", type=Path, required=True)
    parser.add_argument("--inputs", type=Path, required=True)
    parser.add_argument("--stream", choices=("s1", "s2", "s3"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    confirmation, inputs = args.confirmation, args.inputs
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
        "schema": "aic-hierarchical-stability-confirmation-stream-evidence-v1",
        "source_commit": "31555f5",
        "protocol_commit": "f23f3b3c0899c05164231e1e8243b229d2c4d5d2",
        "stream": args.stream,
        "diagnostic_panel_sha256": digest(inputs / "diagnostic-panel.json"),
        "baseline_evidence_sha256": digest(inputs / "baseline-evidence.json"),
        "baseline_completed": sum(baseline.values()),
        "diagnostic_case_ids": case_order,
        "arms": {},
    }
    for arm in "abcd":
        root = confirmation / args.stream / f"arm-{arm}"
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
            common = inputs / "common-panels" / args.stream / f"{generation:06}.json"
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
    easy_start = sum(
        baseline[case_id] for case_id, count in strata.items() if count <= 4
    )
    output["easy_start_completed"] = easy_start
    output["easy_cases"] = sum(count <= 4 for count in strata.values())
    output["replay_contrasts"] = {}
    for replay, control in (("b", "a"), ("d", "c")):
        replay_arm, control_arm = output["arms"][replay], output["arms"][control]
        final = replay_arm["generations"][7]["evaluation"]
        gen5 = replay_arm["generations"][5]["evaluation"]
        easy_final = sum(
            item["completed"] for count, item in final["connection_strata"].items()
            if int(count) <= 4
        )
        lost = final["vs_prior_evaluation"]["lost"]
        output["replay_contrasts"][f"{replay}-{control}"] = {
            "last_two_mean_gain": replay_arm["last_two_mean"] - control_arm["last_two_mean"],
            "last_two_minimum_gain": replay_arm["last_two_minimum"] - control_arm["last_two_minimum"],
            "easy_final_completed": easy_final,
            "easy_final_vs_start": easy_final - easy_start,
            "gen5_completed": gen5["completed"],
            "gen5_to_gen7_lost": lost,
            "easy_and_retention_guard": easy_final >= easy_start - 1
            and 3 * lost <= gen5["completed"],
        }
    args.output.write_text(json.dumps(output, indent=2) + "\n")
    print(f"evidence_bytes={args.output.stat().st_size}", flush=True)


if __name__ == "__main__":
    main()
