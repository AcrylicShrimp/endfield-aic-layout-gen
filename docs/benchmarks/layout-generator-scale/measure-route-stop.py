"""Measure one bounded generator CLI invocation and retain its exact command."""

import json
import resource
import subprocess
import sys
import time
from pathlib import Path


def main() -> None:
    binary, mode, config, seed, output, record = sys.argv[1:7]
    root = Path(__file__).resolve().parents[3]
    command = [
        binary,
        mode,
        "--config",
        str(root / config),
        "--catalog-dir",
        str(root / "data/game/normalized"),
        "--seed",
        seed,
        "--workspace-budget-gib",
        "16",
        "--output",
        output,
    ]
    if mode == "generate":
        command.extend(["--count", "1", "--workers", "1"])
    start = time.monotonic()
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    measurement = {
        "command": command,
        "exit_code": result.returncode,
        "wall_seconds": time.monotonic() - start,
        "max_rss_kib": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
        "stdout": result.stdout.strip(),
        "stderr": result.stderr.strip(),
    }
    Path(record).write_text(json.dumps(measurement, indent=2) + "\n")
    print(json.dumps(measurement))


if __name__ == "__main__":
    main()
