"""Supervise the required W&B observer before and throughout a duration job."""
from pathlib import Path
import subprocess
import sys
import time

from ppo_harness_environment import load_environment, tracking_destination
from ppo_harness_state import read


class TrackingProcess:
    def __init__(self, output, env_file):
        self.output = output
        self.process = None
        protocol = read(output / 'protocol.json')
        config = protocol['config']['tracking']
        if config['backend'] == 'disabled':
            if protocol['config']['purpose'] != 'blueprint-success-duration-readiness':
                raise ValueError('blueprint.duration-tracking: only declared readiness may disable tracking')
            return
        if config['backend'] != 'wandb' or config['mode'] != 'online':
            raise ValueError('blueprint.duration-tracking: expected online W&B configuration')
        env_file = (env_file or Path(__file__).resolve().parents[2] / '.env').resolve()
        load_environment(env_file)
        destination = tracking_destination(required=True)
        if any(config[key] != destination[key] for key in ('entity', 'project')):
            raise ValueError('blueprint.duration-tracking: destination differs from the prepared protocol')
        with (output / 'wandb-observer.log').open('a') as log:
            self.process = subprocess.Popen([sys.executable, str(Path(__file__).with_name('blueprint_duration_tracking.py')),
                '--output', str(output), '--env-file', str(env_file)], stdout=log, stderr=log, stdin=subprocess.DEVNULL)
        try:
            self.wait_for(lambda status: status['status'] in ('ready', 'running'), 'online observer readiness')
        except BaseException:
            self.close()
            raise

    def wait_for(self, predicate, description):
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            self.health()
            path = self.output / 'wandb-observer.json'
            if path.exists():
                status = read(path)
                if status.get('pid') == self.process.pid and predicate(status):
                    return
            time.sleep(.1)
        raise RuntimeError(f'blueprint.duration-tracking: timed out waiting for {description}: {self.output}')

    def health(self, initial=False):
        if self.process is None:
            return
        if self.process.poll() is not None:
            raise RuntimeError(f'blueprint.duration-tracking: observer exited during training; see {self.output / "wandb-observer.json"}')
        if initial:
            self.wait_for(lambda status: status.get('exported_events', 0) >= 1, 'iteration-zero export')

    def finish(self):
        if self.process is not None:
            try:
                code = self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                raise RuntimeError(f'blueprint.duration-tracking: observer did not finish: {self.output}') from None
            if code:
                raise RuntimeError(f'blueprint.duration-tracking: observer exit {code}: {self.output}')

    def close(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
