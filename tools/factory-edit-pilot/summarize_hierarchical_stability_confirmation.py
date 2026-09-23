"""Apply the predeclared three-stream policy-stability confirmation gates."""

import argparse
import json
from pathlib import Path
import statistics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    streams = {}
    for stream in ("s1", "s2", "s3"):
        record = json.loads((args.evidence_dir / f"{stream}.json").read_text())
        if record["stream"] != stream or record["schema"] != (
            "aic-hierarchical-stability-confirmation-stream-evidence-v1"
        ):
            raise ValueError(f"wrong evidence for {stream}")
        streams[stream] = record
    if len({record["diagnostic_panel_sha256"] for record in streams.values()}) != 1:
        raise ValueError("diagnostic panels differ across streams")
    if len({record["baseline_evidence_sha256"] for record in streams.values()}) != 1:
        raise ValueError("common start differs across streams")

    candidates = {}
    for replay, control in (("b", "a"), ("d", "c")):
        rows = [record["replay_contrasts"][f"{replay}-{control}"]
                for record in streams.values()]
        mean_gain = statistics.mean(row["last_two_mean_gain"] for row in rows)
        minimum_gain = statistics.mean(row["last_two_minimum_gain"] for row in rows)
        positive_streams = sum(row["last_two_mean_gain"] > 0 for row in rows)
        guarded_streams = sum(row["easy_and_retention_guard"] for row in rows)
        candidates[replay.upper()] = {
            "matched_control": control.upper(),
            "per_stream": dict(zip(streams, rows)),
            "three_stream_mean_gain": mean_gain,
            "three_stream_minimum_gain": minimum_gain,
            "positive_streams": positive_streams,
            "easy_and_retention_guarded_streams": guarded_streams,
            "passes": mean_gain >= 4 and positive_streams >= 2
            and minimum_gain >= 3 and guarded_streams >= 2,
        }

    passing = [arm for arm in ("B", "D") if candidates[arm]["passes"]]
    if len(passing) == 1:
        selected = passing[0]
    elif len(passing) == 2:
        b_minimum = statistics.mean(record["arms"]["b"]["last_two_minimum"]
                                    for record in streams.values())
        d_minimum = statistics.mean(record["arms"]["d"]["last_two_minimum"]
                                    for record in streams.values())
        selected = "D" if d_minimum - b_minimum >= 3 else "B"
    else:
        selected = None

    result = {
        "schema": "aic-hierarchical-stability-confirmation-summary-v1",
        "diagnostic_panel_sha256": next(iter(streams.values()))["diagnostic_panel_sha256"],
        "streams": list(streams),
        "candidates": candidates,
        "selected_for_integrated_check": selected,
        "decision": "bounded-integrated-check" if selected else "investigate-update-magnitude",
    }
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(f"selected_for_integrated_check={selected}")


if __name__ == "__main__":
    main()
