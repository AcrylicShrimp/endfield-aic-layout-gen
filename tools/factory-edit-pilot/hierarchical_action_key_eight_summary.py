"""Summarize the bounded paired physical-action Gumbel search probe."""

import argparse
import hashlib
import json
from pathlib import Path
import statistics


PAIRS = (
    ("rank-original", "rank-repeat"),
    ("rank-original", "rank-renamed"),
    ("key-original", "key-repeat"),
    ("key-original", "key-renamed"),
    ("rank-original", "key-original"),
)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def physical_key(action):
    kind = action["kind"]
    value = action["value"]
    if kind == "command":
        physical = (kind, value)
    elif kind == "route":
        physical = (kind, value["commands"])
    elif kind == "network":
        physical = (kind, [route["commands"] for route in value["routes"]])
    else:
        raise ValueError(f"unknown macro action kind: {kind}")
    return json.dumps(physical, sort_keys=True, separators=(",", ":"))


def selected(root):
    return physical_key(root["actions"][root["selected_action"]]["action"])


def visited(root):
    return {physical_key(row["action"]) for row in root["actions"]
            if row["visits"] > 0}


def all_actions(root):
    return {physical_key(row["action"]) for row in root["actions"]}


def summarize(root):
    protocol = json.loads((root / "protocol.json").read_text())
    original = json.loads((root / "panel-original.json").read_text())
    renamed = json.loads((root / "panel-renamed.json").read_text())
    assert sha(root / "panel-original.json") == protocol["panel_original_sha256"]
    assert sha(root / "panel-renamed.json") == protocol["panel_renamed_sha256"]
    assert len(original["cases"]) == len(renamed["cases"]) == protocol["cases"]
    assert all(a["id"] != b["id"] and a["problem"] == b["problem"]
               and a["prefix"] == b["prefix"]
               for a, b in zip(original["cases"], renamed["cases"]))

    reports = {}
    runs = []
    for name in protocol["runs"]:
        report_path = root / f"report-{name}.json"
        report = json.loads(report_path.read_text())
        metrics = json.loads((root / f"metrics-{name}.json").read_text())
        assert metrics["report_sha256"] == sha(report_path)
        assert report["status"] == "complete"
        assert len(report["cases"]) == protocol["cases"]
        expected_panel = (protocol["panel_renamed_sha256"] if name.endswith("renamed")
                          else protocol["panel_original_sha256"])
        assert report["panel_sha256"] == expected_panel
        reports[name] = [case["mcts"] for case in report["cases"]]
        runs.append({
            "name": name,
            "report_sha256": metrics["report_sha256"],
            "case_success": "".join("1" if case["fully_validated"] else "0"
                                    for case in reports[name]),
            "completed": sum(case["fully_validated"] for case in reports[name]),
            "root_records": sum(len(case["root_records"]) for case in reports[name]),
            "generated_macro_samples": sum(case["counters"]["generated_macro_samples"]
                                           for case in reports[name]),
            "wall_seconds": metrics["wall_seconds"],
            "maximum_rss_kib": metrics["maximum_rss_kib"],
        })

    pairs = []
    for left, right in PAIRS:
        cases = list(zip(reports[left], reports[right]))
        first_overlap = [len(visited(a["root_records"][0])
                             & visited(b["root_records"][0])) for a, b in cases]
        all_overlap = [len(all_actions(a["root_records"][0])
                           & all_actions(b["root_records"][0])) for a, b in cases]
        pairs.append({
            "left": left,
            "right": right,
            "first_root_visited_overlap": first_overlap,
            "first_root_mean_visited_overlap": statistics.mean(first_overlap),
            "first_root_all_action_overlap": all_overlap,
            "same_first_physical_action": sum(
                selected(a["root_records"][0]) == selected(b["root_records"][0])
                for a, b in cases),
            "same_full_physical_path": sum(
                [selected(root) for root in a["root_records"]]
                == [selected(root) for root in b["root_records"]]
                for a, b in cases),
            "same_completion": sum(a["fully_validated"] == b["fully_validated"]
                                   for a, b in cases),
            "completion_gains": sum(not a["fully_validated"] and b["fully_validated"]
                                    for a, b in cases),
            "completion_losses": sum(a["fully_validated"] and not b["fully_validated"]
                                     for a, b in cases),
        })

    return {
        "schema": "aic-hierarchical-action-key-eight-summary-v1",
        "protocol_sha256": sha(root / "protocol.json"),
        "runs": runs,
        "pairs": pairs,
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = summarize(args.output)
    write(args.output / "summary.json", result)
    for pair in result["pairs"]:
        print(pair["left"], pair["right"],
              pair["first_root_mean_visited_overlap"],
              pair["same_first_physical_action"],
              pair["completion_gains"], pair["completion_losses"])
