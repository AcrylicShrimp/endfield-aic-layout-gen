"""Durable fresh-factory PPO experiments with supervised execution and W&B."""
import argparse
import json
from pathlib import Path

from ppo_harness_environment import load_environment
from ppo_harness_protocol import prepare
from ppo_harness_state import diagnostic, writer_lock, load_state, require
from ppo_harness_supervisor import supervise, stop_run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file', type=Path, default=Path(__file__).resolve().parents[2] / '.env')
    commands = parser.add_subparsers(dest='command', required=True)
    freeze = commands.add_parser('prepare')
    freeze.add_argument('--config', type=Path, required=True)
    freeze.add_argument('--output', type=Path, required=True)
    for name in ('run', 'resume', 'status', 'stop', 'plot', 'export'):
        command = commands.add_parser(name)
        command.add_argument('--run', type=Path, required=True)
        if name in ('run', 'resume'):
            command.add_argument('--duration-seconds', type=float,
                                 help='End this training session normally after this duration; preserve the last committed batch')
        if name == 'plot':
            command.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        load_environment(args.env_file)
        if args.command == 'prepare':
            protocol = prepare(args.config, args.output)
            result = dict(status='prepared', jobs=len(protocol['jobs']), phases_per_job=len(protocol['phases']))
        elif args.command in ('run', 'resume'):
            result = supervise(args.run, resume=args.command == 'resume', duration_seconds=args.duration_seconds)
        elif args.command == 'stop':
            result = stop_run(args.run)
        elif args.command == 'export':
            from ppo_harness_monitor import watch
            import uuid
            with writer_lock(args.run.resolve()):
                require(load_state(args.run.resolve())['status'] in ('complete', 'stopped', 'failed'),
                        'export-active', 'export requires a terminal run; the live observer owns active export', 'export', args.run)
                watch(args.run.resolve(), uuid.uuid4().hex)
            result = dict(status='exported')
        else:
            from ppo_harness_monitor import status, plot
            if args.command == 'status':
                result = status(args.run.resolve())
            else:
                plot(args.run.resolve(), args.output)
                result = dict(status='rendered', output=str(args.output.resolve()))
        print(json.dumps(result, allow_nan=False), flush=True)
        return 1 if result['status'] == 'failed' else 0
    except Exception as error:
        print(json.dumps(dict(status='failed', diagnostic=diagnostic(error, args.command))), flush=True)
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
