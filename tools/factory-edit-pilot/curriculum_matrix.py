"""Execute and audit the declared curriculum/parallel matrix sequentially, preserving each run's log."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from evidence import write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--protocol", type=Path, required=True)
    parser.add_argument("--native-library", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root, protocol_path, output = args.root.resolve(), args.protocol.resolve(), args.output.resolve()
    if output.exists():
        parser.error(f"output must be fresh: {output}")
    protocol = json.loads(protocol_path.read_text())
    source = subprocess.run(["git", "rev-parse", "HEAD"], cwd=root, capture_output=True, text=True, check=True).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain"], cwd=root, capture_output=True, text=True, check=True).stdout
    if dirty:
        parser.error("matrix requires a clean committed checkout before recording source provenance")
    output.mkdir(parents=True)
    directory = root / "tools/factory-edit-pilot"
    state = {"schema_version": 1, "status": "running", "source_commit": source, "runs": []}
    paths = []
    started = time.perf_counter()
    try:
        for seed in protocol["training_seeds"]:
            for condition in protocol["conditions"]:
                name = f"{condition}-seed{seed}"
                path = output / name
                row = {"condition": condition, "seed": seed, "directory": str(path), "status": "running"}
                state["runs"].append(row)
                write(output / "matrix.json", state)
                print(json.dumps({"event": "matrix_run_started", **row}), flush=True)
                with (output / f"{name}.log").open("w") as log:
                    subprocess.run([sys.executable, str(directory / "curriculum_study.py"), "--root", str(root),
                                    "--protocol", str(protocol_path), "--condition", condition, "--seed", str(seed),
                                    "--native-library", str(args.native_library.resolve()), "--output", str(path)],
                                   cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
                    subprocess.run([sys.executable, str(directory / "curriculum_audit.py"), "--root", str(root),
                                    "--protocol", str(protocol_path), "--output", str(path), "--source-commit", source],
                                   cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
                row["status"] = "complete_and_audited"
                paths.append(path)
                write(output / "matrix.json", state)
                print(json.dumps({"event": "matrix_run_complete", **row}), flush=True)
        subprocess.run([sys.executable, str(directory / "curriculum_compare.py"), "--protocol", str(protocol_path),
                        "--runs", *[str(path) for path in paths], "--output", str(output / "comparison")], cwd=root, check=True)
        state["status"] = "complete"
    except BaseException as error:
        state.update(status="interrupted", error={"code": "study_matrix.interrupted", "severity": "error",
                     "stage": "study_matrix", "entity": str(output), "message": str(error) or type(error).__name__})
        raise
    finally:
        state["wall_seconds"] = time.perf_counter() - started
        write(output / "matrix.json", state)


if __name__ == "__main__":
    main()
