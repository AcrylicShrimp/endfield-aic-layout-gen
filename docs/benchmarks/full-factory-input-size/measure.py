"""Measure a precompiled native input/checkpoint size probe with process RSS."""

import argparse
import json
import resource
import subprocess
import sys
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--witness", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source = args.source.resolve()
    witness = args.witness.resolve()
    output = args.output.resolve()
    binaries = [
        path
        for path in (source / "target/release/deps").glob("aic_pilot_native-*")
        if path.is_file() and path.stat().st_mode & 0o111
    ]
    if len(binaries) != 1:
        parser.error(f"expected one precompiled test binary, found {binaries}")
    command = [
        str(binaries[0]),
        "measure_generated_input_and_checkpoint_bytes",
        "--ignored",
        "--nocapture",
    ]
    started = time.monotonic()
    completed = subprocess.run(
        command,
        cwd=source,
        env={"PATH": "/usr/bin:/bin", "AIC_PLAN24_WITNESS": str(witness)},
        capture_output=True,
        text=True,
        timeout=180,
    )
    rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    output.parent.mkdir(parents=True, exist_ok=True)
    output.with_suffix(".log").write_text(completed.stdout + completed.stderr)
    summary = {
        "exit_code": completed.returncode,
        "wall_s": time.monotonic() - started,
        "maxrss_kib": rss // 1024 if sys.platform == "darwin" else rss,
        "source": str(source),
        "witness": str(witness),
        "command": command,
    }
    output.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))
    if completed.returncode:
        print("\n".join((completed.stdout + completed.stderr).splitlines()[-30:]), file=sys.stderr)
        sys.exit(completed.returncode)


if __name__ == "__main__":
    main()
