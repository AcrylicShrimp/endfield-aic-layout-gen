"""Run the detached training job, then audit its last two durable updates."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import subprocess
import sys

import torch

from audit_blueprint_duration import audit_run
from blueprint_duration import run_locked
from blueprint_duration_store import exclusive_run
from blueprint_ppo import check_sources
from blueprint_ppo_store import read, reference, sha, write
from blueprint_ppo_update import require
from blueprint_duration_tracking_job import TrackingProcess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--env-file', type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    with exclusive_run(output):
        execute(output, args.env_file)


def execute(output, env_file=None):
    protocol, state = read(output / 'protocol.json'), read(output / 'state.json')
    require(protocol['schema'] == state['schema'] == 'blueprint-success-duration-v2',
            'blueprint.duration: v2 required; historical v1 resume requires its frozen runner')
    require(state['protocol_sha256'] == sha(output / 'protocol.json'), 'blueprint.duration: protocol binding')
    require(state['status'] != 'complete', 'blueprint.duration: already complete')
    check_sources(protocol)
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    awake = None
    tracker = None
    result = dict(schema='blueprint-duration-completion-v2', pid=os.getpid(), status='running',
                  started_at=datetime.now(timezone.utc).isoformat())
    write(output / 'completion.json', result)
    try:
        if sys.platform == 'darwin':
            awake = subprocess.Popen(['/usr/bin/caffeinate', '-i', '-w', str(os.getpid())],
                                     stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        tracker = TrackingProcess(output, env_file)
        run_locked(output, health=tracker.health)
        tracker.finish()
        audit_run(output, full=False)
        state = read(output / 'state.json')
        result.update(status=state['status'], stop_reason=state['stop_reason'],
                      state=reference(output / 'state.json', output), tail_audit=reference(output / 'audit.json', output))
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        result['finished_at'] = datetime.now(timezone.utc).isoformat()
        write(output / 'completion.json', result)
        if tracker is not None:
            tracker.close()
        if awake is not None and awake.poll() is None:
            awake.terminate()
            awake.wait(timeout=10)


if __name__ == '__main__':
    main()
