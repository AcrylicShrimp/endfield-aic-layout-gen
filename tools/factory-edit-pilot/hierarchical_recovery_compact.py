"""Retain bounded case masks and mechanism summaries before raw search cleanup."""

import argparse
from pathlib import Path

from hierarchical_recovery_search import ARMS, SEEDS
from ppo_harness_state import read, sha, write


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-recovery-compact: {message}")


def compact(root):
    summary = read(root / "summary.json")
    alignment = read(root / "action-alignment.json")
    protocol = read(root / "protocol.json")
    progress = read(root / "progress.json")
    require(progress["status"] == "complete"
            and progress["protocol_sha256"] == sha(root / "protocol.json")
            and summary["progress_sha256"] == sha(root / "progress.json")
            and alignment["progress_sha256"] == sha(root / "progress.json")
            and len(summary["cases"]) == 48
            and len(progress["records"]) == protocol["planned_cells"],
            "source reports not final")
    cases = []
    for row in summary["cases"]:
        base = row["arms"]["base"]
        require(all(set(row["arms"][arm]) == {str(seed) for seed in SEEDS}
                    for arm in ARMS), "seed results missing")
        masks = {}
        first_matches = {}
        for arm in ARMS:
            masks[arm] = sum((1 << index) if row["arms"][arm][str(seed)]["success"]
                             else 0 for index, seed in enumerate(SEEDS))
            first_matches[arm] = sum(
                (1 << index) if row["arms"][arm][str(seed)]["first_action_sha256"]
                == base[str(seed)]["first_action_sha256"] else 0
                for index, seed in enumerate(SEEDS))
        cases.append({"case_id": row["case_id"], "stream": row["stream"],
                      "success_masks": masks,
                      "first_action_equal_base_masks": first_matches})
    require(len({case["case_id"] for case in cases}) == 48, "duplicate case")
    aligned = {arm: {key: value for key, value in metric.items()
                     if key != "case_rows"}
               for arm, metric in alignment["results"].items()}
    result = {
        "schema": "aic-hierarchical-recovery-search-compact-v1",
        "runner_sha256": sha(Path(__file__)),
        "protocol_sha256": sha(root / "protocol.json"),
        "progress_sha256": sha(root / "progress.json"),
        "full_summary_sha256": sha(root / "summary.json"),
        "full_alignment_sha256": sha(root / "action-alignment.json"),
        "success_mask_bit_order": list(SEEDS),
        "summary": summary["summary"],
        "action_alignment": aligned,
        "cases": cases,
    }
    write(root / "compact.json", result)
    require((root / "compact.json").stat().st_size <= 100 * 1024,
            "compact evidence exceeds documentation size limit")
    print(f"compact_cases={len(cases)} bytes={(root / 'compact.json').stat().st_size}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    compact(args.output)


if __name__ == "__main__":
    main()
