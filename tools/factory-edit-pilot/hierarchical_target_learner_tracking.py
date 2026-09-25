"""Mirror committed target-learner results to W&B without blocking training."""

import argparse
import fcntl
import hashlib
import os
from pathlib import Path
import signal
import threading
import time

from ppo_harness_environment import load_environment, tracking_destination
from ppo_harness_state import read, sha, write


def require(condition, message):
    if not condition:
        raise ValueError(f"hierarchical-target-learner-tracking: {message}")


def remote_step(wandb, destination, identifier):
    return wandb.Api(timeout=15).run(
        f"{destination['entity']}/{destination['project']}/{identifier}").lastHistoryStep


def observe(directory, env_file, stopped):
    load_environment(env_file)
    destination = tracking_destination(required=True)
    import wandb

    protocol_sha = sha(directory / "protocol.json")
    protocol = read(directory / "protocol.json")
    require(protocol["schema"] == "aic-hierarchical-target-learner-protocol-v1"
            and protocol["phase"] in ("screen", "confirmation"), "wrong protocol")
    maximum = protocol["planned_records"]
    identifier = hashlib.sha256(
        f"hierarchical-target-learner:{protocol_sha}".encode()).hexdigest()[:24]
    status_path = directory / "wandb-observer.json"
    status = {"schema": "aic-hierarchical-target-learner-wandb-v1",
              "protocol_sha256": protocol_sha, "run_id": identifier,
              "status": "starting", "pid": os.getpid()}
    write(status_path, status)
    run = None
    try:
        telemetry = directory / "wandb-telemetry"
        telemetry.mkdir(exist_ok=True)
        run = wandb.init(**destination, id=identifier, resume="allow", mode="online",
            name=f"hierarchical-target-learner-{protocol['phase']}-20260925",
            group="hierarchical-target-causal", job_type=protocol["phase"],
            dir=str(telemetry), save_code=False,
            settings=wandb.Settings(console="off", disable_code=True, x_disable_meta=True,
                x_disable_stats=True, x_disable_machine_info=True,
                init_timeout=30, finish_timeout=20),
            config={"protocol_sha256": protocol_sha,
                    "phase": protocol["phase"], "contrast": protocol["contrast"],
                    "planned_records": maximum})
        acknowledged = remote_step(wandb, destination, identifier)
        if acknowledged < 0:
            run.log({"event": "readiness", "planned_records": maximum}, step=0,
                    commit=True)
            for _ in range(60):
                if stopped.wait(1):
                    raise RuntimeError("stopped before readiness acknowledgement")
                acknowledged = remote_step(wandb, destination, identifier)
                if acknowledged >= 0:
                    break
        require(acknowledged >= 0, "W&B readiness record was not acknowledged")
        status.update(status="ready", url=run.url, remote_confirmed_step=acknowledged,
                      exported_records=min(acknowledged, maximum))
        write(status_path, status)
        cursor = min(acknowledged, maximum)
        while True:
            progress = read(directory / "progress.json")
            require(progress["protocol_sha256"] == protocol_sha,
                    "progress protocol changed")
            rows = progress["records"]
            require(cursor <= len(rows) <= maximum, "record prefix changed")
            for index in range(cursor, len(rows)):
                row = rows[index]
                run.log({"stream": row["stream"], "arm": row["arm"],
                         "generation": row.get("generation", 1),
                         "diagnostic/completed": row["completed"],
                         "diagnostic/gained_from_start": row["gained_from_start"],
                         "diagnostic/lost_from_start": row["lost_from_start"],
                         "diagnostic/easy_completed": row["easy_completed"],
                         "diagnostic/hard_completed": row["hard_completed"],
                         "train/initial_loss": row["initial_outer_loss"],
                         "train/final_loss": row["final_outer_loss"]},
                        step=index + 1, commit=True)
            cursor = len(rows)
            run.summary.update({"source_status": progress["status"],
                                "exported_records": cursor,
                                "observer_heartbeat_epoch": time.time()})
            status.update(status="running", exported_records=cursor,
                          source_status=progress["status"])
            write(status_path, status)
            if progress["status"] == "complete":
                require(cursor == maximum, "complete source has too few records")
                break
            require(progress["status"] in ("prepared", "running"),
                    "source progress failed")
            if stopped.wait(2):
                status.update(status="detached", finished_epoch=time.time())
                write(status_path, status)
                run.finish(exit_code=1)
                return
        run.finish(exit_code=0)
        run = None
        for _ in range(12):
            acknowledged = remote_step(wandb, destination, identifier)
            if acknowledged >= maximum:
                break
            time.sleep(2)
        require(acknowledged >= maximum, "final W&B history is not acknowledged")
        status.update(status="finished", remote_confirmed_step=acknowledged,
                      finished_epoch=time.time())
        write(status_path, status)
    except BaseException as error:
        message = str(error)
        secret = os.environ.get("WANDB_API_KEY")
        if secret:
            message = message.replace(secret, "[redacted]")
        status.update(status="failed", error=message, finished_epoch=time.time())
        write(status_path, status)
        if run is not None:
            run.finish(exit_code=1)
        raise RuntimeError(message) from None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--env-file", type=Path, required=True)
    args = parser.parse_args()
    directory = args.output.resolve()
    stopped = threading.Event()
    for number in (signal.SIGTERM, signal.SIGINT):
        signal.signal(number, lambda *_: stopped.set())
    with (directory / ".wandb-observer.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        observe(directory, args.env_file, stopped)


if __name__ == "__main__":
    main()
