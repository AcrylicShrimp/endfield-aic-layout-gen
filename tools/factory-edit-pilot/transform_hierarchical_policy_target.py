"""Create a provenance-linked target-only research collection from a search report."""

import argparse
from pathlib import Path

from hierarchical_target_contrast import targets
from ppo_harness_state import read, sha, write


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-target-transform: {message}")


def transform(source, output, manifest, variant, scope):
    require(variant in ("q2", "q4", "half-visited")
            and scope in ("first", "all"), "invalid transform or scope")
    require(source.is_file() and not output.exists() and not manifest.exists(),
            "source missing or output already exists")
    report = read(source)
    require(report["schema"] == "aic-hierarchical-search-report-v7"
            and report["partition"] == "training"
            and report["root_records_retained"]
            and report["status"] == "complete", "invalid frozen search report")
    roots = 0
    actions = 0
    total_variation = 0.0
    for case in report["cases"]:
        require(case["mcts"]["root_records"],
                f"case has no roots: {case['id']}")
        for move, root in enumerate(case["mcts"]["root_records"]):
            if scope == "first" and move != 0:
                continue
            original = [action["policy_target"] for action in root["actions"]]
            _, _, alternatives = targets(root["actions"])
            replacement = alternatives[variant]
            require(len(replacement) == len(original),
                    f"target length differs: {case['id']}/{move}")
            for action, probability in zip(root["actions"], replacement):
                action["policy_target"] = probability
            roots += 1
            actions += len(replacement)
            total_variation += 0.5 * sum(abs(left - right)
                                         for left, right in zip(original, replacement))
    require(roots > 0, "nothing transformed")
    output.parent.mkdir(parents=True, exist_ok=True)
    manifest.parent.mkdir(parents=True, exist_ok=True)
    write(output, report)
    write(manifest, {
        "schema": "aic-hierarchical-target-transform-v1",
        "source": str(source), "source_sha256": sha(source),
        "output": str(output), "output_sha256": sha(output),
        "variant": variant, "scope": scope, "modified_roots": roots,
        "modified_actions": actions,
        "mean_root_total_variation": total_variation / roots,
        "search_config_sha256": report["search_config_sha256"],
        "panel_sha256": report["panel_sha256"],
        "lower_checkpoint_sha256": report["lower_checkpoint_sha256"],
        "outer_checkpoint_sha256": report["outer_checkpoint_sha256"],
    })
    print(f"transformed_roots={roots} actions={actions} mean_tv={total_variation / roots:.6f}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collection", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--variant", choices=("q2", "q4", "half-visited"), required=True)
    parser.add_argument("--scope", choices=("first", "all"), required=True)
    args = parser.parse_args()
    transform(args.collection, args.output, args.manifest, args.variant, args.scope)


if __name__ == "__main__":
    main()
