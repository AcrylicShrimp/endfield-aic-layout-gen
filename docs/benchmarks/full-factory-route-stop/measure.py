"""Run the precompiled full-factory route panel with process time and RSS."""

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
    parser.add_argument("--test", choices=("measure_region_route_panel", "measure_region_action_chain_counts", "measure_region_chain_capacity", "measure_region_default_panel"), default="measure_region_route_panel")
    parser.add_argument("--work", type=int)
    parser.add_argument("--attempt-work", type=int)
    parser.add_argument("--attempts", type=int)
    parser.add_argument("--port-trials", type=int)
    parser.add_argument("--extra-chains", type=int)
    parser.add_argument("--detail", action="store_true")
    args = parser.parse_args()
    source = args.source.resolve()
    witness = args.witness.resolve()
    output = args.output.resolve()
    binaries = list((source / "target/release/deps").glob("aic_pilot_native-*"))
    binaries = [path for path in binaries if path.is_file() and path.stat().st_mode & 0o111]
    if len(binaries) != 1:
        parser.error(f"expected one precompiled test binary, found {binaries}")
    command = [str(binaries[0]), args.test, "--ignored", "--nocapture"]
    started = time.monotonic()
    environment = {"PATH": "/usr/bin:/bin", "AIC_REGION_WITNESS": str(witness)}
    if args.work is not None:
        environment["AIC_REGION_WORK"] = str(args.work)
    if args.attempt_work is not None:
        environment["AIC_REGION_ATTEMPT_WORK"] = str(args.attempt_work)
    if args.attempts is not None:
        environment["AIC_REGION_ATTEMPTS"] = str(args.attempts)
    if args.port_trials is not None:
        environment["AIC_REGION_PORT_TRIALS"] = str(args.port_trials)
    if args.extra_chains is not None:
        environment["AIC_REGION_EXTRA_CHAINS"] = str(args.extra_chains)
    if args.detail:
        environment["AIC_REGION_DETAIL"] = "1"
    completed = subprocess.run(
        command,
        cwd=source,
        env=environment,
        capture_output=True,
        text=True,
        timeout=600,
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
