"""Explicit admission and execution commands for the registered replay screen."""
import argparse
from copy import deepcopy
import json
from pathlib import Path

import torch

from blueprint_ppo_store import read
from blueprint_replay_contract import SCHEMA, check, diagnostic


def specification(path, parent_matrix):
    value = read(path)
    check(value['schema'] == SCHEMA, 'source-invalid', 'configuration', 'expected the replay-balance v1 specification', path)
    value = deepcopy(value)
    marker = '@matrix-parent/'
    if value['import_selection'].startswith(marker):
        value['import_selection'] = str(Path(parent_matrix).resolve().parent / value['import_selection'][len(marker):])
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    admit = commands.add_parser('admit', help='Authenticate and cache the parent replay and five imported successes once')
    admit.add_argument('--spec', type=Path, default=Path(__file__).with_name('blueprint-replay-balance.json'))
    admit.add_argument('--parent-matrix', type=Path, required=True)
    admit.add_argument('--output', type=Path, required=True)
    ready = commands.add_parser('readiness', help='Measure the registered serial/CPU/CUDA complete cycles')
    ready.add_argument('--spec', type=Path, default=Path(__file__).with_name('blueprint-replay-balance.json'))
    ready.add_argument('--parent-matrix', type=Path, required=True)
    ready.add_argument('--sources', type=Path, required=True)
    ready.add_argument('--output', type=Path, required=True)
    verify = commands.add_parser('verify-fork', help='Run the bounded restored fork and actual CUDA profiler')
    verify.add_argument('--spec', type=Path, default=Path(__file__).with_name('blueprint-replay-balance.json'))
    verify.add_argument('--parent-matrix', type=Path, required=True)
    verify.add_argument('--sources', type=Path, required=True)
    verify.add_argument('--measurement', type=Path, required=True)
    verify.add_argument('--output', type=Path, required=True)
    panel = commands.add_parser('prepare-panel', help='Freeze sixteen native-valid unseen large initial geometries')
    panel.add_argument('--spec', type=Path, default=Path(__file__).with_name('blueprint-replay-balance.json'))
    panel.add_argument('--parent-matrix', type=Path, required=True)
    panel.add_argument('--sources', type=Path, required=True)
    panel.add_argument('--output', type=Path, required=True)
    prepare = commands.add_parser('prepare', help='Bind the measured runtime, all arms and evaluation panels')
    prepare.add_argument('--spec', type=Path, default=Path(__file__).with_name('blueprint-replay-balance.json'))
    prepare.add_argument('--parent-matrix', type=Path, required=True)
    prepare.add_argument('--sources', type=Path, required=True)
    prepare.add_argument('--measurement', type=Path, required=True)
    prepare.add_argument('--verification', type=Path, required=True)
    prepare.add_argument('--panel', type=Path)
    prepare.add_argument('--output', type=Path, required=True)
    run = commands.add_parser('run', help='Execute or restore the bound fifteen-arm matrix')
    run.add_argument('--output', type=Path, required=True)
    run.add_argument('--environment', type=Path, required=True)
    probe = commands.add_parser('probe', help='Run the separately bounded autonomous anchor continuation diagnostic')
    probe.add_argument('--matrix', type=Path, required=True)
    probe.add_argument('--output', type=Path, required=True)
    report = commands.add_parser('report', help='Compute the registered paired contrasts and shared transfer subset')
    report.add_argument('--matrix', type=Path, required=True)
    report.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    try:
        if args.command == 'admit':
            from blueprint_replay_source import prepare_sources
            result = prepare_sources(args.parent_matrix, args.output, specification(args.spec, args.parent_matrix))
            print(json.dumps(dict(status='complete', output=str(args.output), parents=result['parents'], imported=len(result['imported_ids']))))
        elif args.command == 'readiness':
            from blueprint_replay_readiness import run_readiness
            result = run_readiness(args.sources, args.output, specification(args.spec, args.parent_matrix))
            print(json.dumps(dict(status='measured', device=result['device'], speedup=result['speedup'], forecast_seconds=result['forecast_seconds'])))
        elif args.command == 'verify-fork':
            from blueprint_replay_verify import verify_fork
            result = verify_fork(args.sources, args.measurement, args.output, specification(args.spec, args.parent_matrix))
            print(json.dumps(dict(status=result['status'], device=result['device'], exact_update_replay=result['exact_update_replay'])))
        elif args.command == 'prepare-panel':
            from blueprint_execution_resources import cpu_allocation
            from blueprint_evaluation_cohort import load_cohort
            from blueprint_ppo_store import ROOT
            from blueprint_replay_panel import prepare_panel
            from blueprint_replay_runtime import admitted_sources
            parent, imports, _ = admitted_sources(args.sources, 17)
            config = parent['parent_protocol']['config']
            probe = read(ROOT / config['probe_spec'])
            historical, _ = load_cohort(config['evaluation']['cohort'], 32,
                {k: ROOT / p for k, p in probe['catalogs'].items()})
            result = prepare_panel(parent, imports, historical, args.output,
                specification(args.spec, args.parent_matrix)['transfer_panel'], cpu_allocation(8)['workers'])
            print(json.dumps(dict(status='complete', cases=len(result['cases']), attempts=len(result['attempts']))))
        elif args.command == 'prepare':
            from blueprint_replay_launch import prepare
            result = prepare(args.sources, args.measurement, args.verification, args.output,
                specification(args.spec, args.parent_matrix), args.panel)
            print(json.dumps(dict(status='prepared', device=result['device'], arms=len(result['order']))))
        elif args.command == 'run':
            from blueprint_replay_run import run
            run(args.output, args.environment)
        elif args.command == 'probe':
            from blueprint_replay_probe import run_probe
            result = run_probe(args.matrix, args.output)
            print(json.dumps(dict(status=result['status'], records=len(result['records']), seconds=result['seconds'])))
        elif args.command == 'report':
            from blueprint_replay_report import build_report
            result = build_report(args.matrix, args.output)
            print(json.dumps(dict(status=result['status'], eligible_cases=result['eligible_cases'], gates=result['gates'])))
    except Exception as error:
        print(json.dumps(dict(status='failed', diagnostic=diagnostic(error, args.command))))
        raise SystemExit(1) from None


if __name__ == '__main__':
    main()
