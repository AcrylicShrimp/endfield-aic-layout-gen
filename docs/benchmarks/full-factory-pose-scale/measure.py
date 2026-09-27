"""Measure the ignored full-factory vocabulary test after precompilation."""

import argparse
import json
import os
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
    parser.add_argument("--execute-first", action="store_true")
    args = parser.parse_args()
    source = args.source.resolve()
    witness = args.witness.resolve()
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["AIC_POSE_VOCABULARY_WITNESS"] = str(witness)
    if args.execute_first:
        environment["AIC_POSE_EXECUTE_FIRST"] = "1"
    command = ["cargo", "test", "--release", "-p", "aic-pilot-native",
               "measure_pose_vocabulary_witness", "--", "--ignored", "--nocapture"]
    started = time.monotonic()
    try:
        completed = subprocess.run(command, cwd=source, env=environment,
                                   capture_output=True, text=True, timeout=600)
        status = completed.returncode
        log = completed.stdout + completed.stderr
    except subprocess.TimeoutExpired as error:
        status = "timeout"
        log = (error.stdout or b"").decode(errors="replace") + (
            error.stderr or b"").decode(errors="replace")
    raw_rss = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    rss_kib = raw_rss // 1024 if sys.platform == "darwin" else raw_rss
    summary = {"status": status, "wall_s": time.monotonic() - started,
               "maxrss_kib": rss_kib, "witness": witness.name,
               "source": str(source), "command": command}
    output.with_suffix(".log").write_text(log)
    output.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))
    if status != 0:
        print("\n".join(log.splitlines()[-30:]), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
