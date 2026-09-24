"""Mirror committed update-budget probe results to W&B without blocking work."""

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
        raise ValueError(f"hierarchical-update-budget.tracking: {message}")


def remote_step(wandb, destination, identifier):
    path = f"{destination['entity']}/{destination['project']}/{identifier}"
    return wandb.Api(timeout=15).run(path).lastHistoryStep


def observe(root, env_file, stopped):
    load_environment(env_file)
    destination = tracking_destination(required=True)
    import wandb

    protocol_sha = sha(root / "protocol.json")
    protocol = read(root / "protocol.json")
    require(protocol["schema"] == "aic-hierarchical-update-budget-probe-v1", "wrong protocol")
    identifier = hashlib.sha256(f"hierarchical-update-budget:{protocol_sha}".encode()).hexdigest()[:24]
    status_path = root / "wandb-observer.json"
    status = {
        "schema": "aic-hierarchical-update-budget-wandb-v1",
        "protocol_sha256": protocol_sha,
        "run_id": identifier,
        "status": "starting",
        "pid": os.getpid(),
    }
    write(status_path, status)
    run = None
    try:
        telemetry = root / "wandb-telemetry"
        telemetry.mkdir(exist_ok=True)
        run = wandb.init(**destination, id=identifier, resume="allow", mode="online",
            name="hierarchical-update-budget-20260925", group="hierarchical-policy-stability",
            job_type="update-budget-probe", dir=str(telemetry), save_code=False,
            settings=wandb.Settings(console="off", disable_code=True, x_disable_meta=True,
                x_disable_stats=True, x_disable_machine_info=True,
                init_timeout=30, finish_timeout=20),
            config={"protocol_sha256": protocol_sha,
                    "experiment_commit": protocol["experiment_commit"],
                    "training_source_commit": protocol["training_source_commit"],
                    "budgets_epochs": protocol["budgets_epochs"],
                    "streams": protocol["streams"]})
        acknowledged = remote_step(wandb, destination, identifier)
        if acknowledged < 0:
            run.log({"event": "readiness", "planned_records": 12}, step=0, commit=True)
            for _ in range(60):
                if stopped.wait(1):
                    raise RuntimeError("stopped before readiness acknowledgement")
                acknowledged = remote_step(wandb, destination, identifier)
                if acknowledged >= 0:
                    break
        require(acknowledged >= 0, "readiness record was not acknowledged")
        status.update(status="ready", url=run.url, remote_confirmed_step=acknowledged,
                      exported_records=min(acknowledged, 12))
        write(status_path, status)
        cursor = min(acknowledged, 12)
        while True:
            progress = read(root / "progress.json")
            require(progress["protocol_sha256"] == protocol_sha, "progress protocol changed")
            records = progress["records"]
            require(cursor <= len(records) <= 12, "record prefix changed")
            for index in range(cursor, len(records)):
                row = records[index]
                run.log({
                    "stream": row["stream"],
                    "epochs": row["epochs"],
                    "updates": row["updates"],
                    "diagnostic/completed": row["completed"],
                    "diagnostic/gained_from_start": row["gained_vs_start"],
                    "diagnostic/lost_from_start": row["lost_vs_start"],
                    "diagnostic/easy_completed": row["easy_completed"],
                    "train/initial_loss": row["initial_outer_loss"],
                    "train/final_loss": row["final_outer_loss"],
                }, step=index + 1, commit=True)
            cursor = len(records)
            run.summary.update({"source_status": progress["status"],
                                "exported_records": cursor,
                                "observer_heartbeat_epoch": time.time()})
            status.update(status="running", exported_records=cursor,
                          source_status=progress["status"])
            write(status_path, status)
            if progress["status"] == "complete":
                require(cursor == 12, "complete source has fewer than 12 records")
                break
            require(progress["status"] in ("prepared", "running"), "source failed")
            if stopped.wait(2):
                status.update(status="detached", finished_epoch=time.time())
                write(status_path, status)
                run.finish(exit_code=1)
                return
        run.finish(exit_code=0)
        run = None
        for _ in range(12):
            acknowledged = remote_step(wandb, destination, identifier)
            if acknowledged >= 12:
                break
            time.sleep(2)
        require(acknowledged >= 12, "final W&B history has not been acknowledged")
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
    root = args.output.resolve()
    stopped = threading.Event()
    for number in (signal.SIGTERM, signal.SIGINT):
        signal.signal(number, lambda *_: stopped.set())
    with (root / ".wandb-observer.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        observe(root, args.env_file, stopped)


if __name__ == "__main__":
    main()
