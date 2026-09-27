"""Verify every historical action survives unchanged and in relative order."""

import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--current", type=Path, required=True)
    args = parser.parse_args()
    baseline = json.loads(args.baseline.read_text())
    current = json.loads(args.current.read_text())
    cursor = 0
    for action in current:
        if cursor < len(baseline) and action == baseline[cursor]:
            cursor += 1
    print(json.dumps({"baseline_actions": len(baseline),
                      "current_actions": len(current),
                      "retained_in_order": cursor}))
    if cursor != len(baseline):
        raise SystemExit("Historical action or relative order changed")


if __name__ == "__main__":
    main()
