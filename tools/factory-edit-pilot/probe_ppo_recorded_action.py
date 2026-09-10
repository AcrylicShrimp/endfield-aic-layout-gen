"""Replay one recorded PPO edit and vary only its local routing work allowance."""
import argparse
import gzip
import json
from pathlib import Path
import time

from native_factory_runtime import FactoryRuntime
from ppo_harness_state import check_sources, checked_path, read, require, sha, write


def match(actual, expected, isolated=False):
    keys = ('action', 'executor_status', 'current_sha256', 'route_expansions')
    if not isolated:
        keys += ('status', 'best_sha256', 'best_score')
    for key in keys:
        require(actual[key] == expected[key], 'probe-replay-mismatch',
                f'recorded action {expected["selected"]} differs: {key}', 'probe')


def run(args):
    root, output = args.run.resolve(), args.output.resolve()
    protocol, state = read(root / 'protocol.json'), read(root / 'state.json')
    check_sources(protocol)
    jobs = [job for job in state['jobs'] if job['id'] == args.job]
    require(len(jobs) == 1, 'probe-job', 'expected one matching recorded job', 'probe', args.job)
    records = [ref for ref in jobs[0]['records']
               if ref['phase'] == dict(kind=args.phase, batch=args.batch)]
    require(len(records) == 1, 'probe-phase', 'expected one committed phase', 'probe', root)
    record_path = checked_path(root, records[0])
    matches = [result for result in read(record_path)['results']
               if result['identity']['index'] == args.episode_index]
    require(len(matches) == 1 and matches[0]['status'] == 'complete',
            'probe-episode', 'expected one completed episode', 'probe', args.episode_index)
    result = matches[0]
    require(sha(result['result_path']) == result['result_sha256'],
            'probe-result', 'recorded result bytes changed', 'probe', result['result_path'])
    episode_path = Path(result['row']['directory']) / 'episode.json.gz'
    with gzip.open(episode_path, 'rt') as stream:
        trace = json.load(stream)['trace']
    require(1 <= args.action_number <= len(trace), 'probe-action',
            'action number must be in the recorded trajectory', 'probe', args.action_number)
    require(trace[args.action_number - 1]['action']['kind'] != 'restore_best',
            'probe-restore', 'restore_best is not a routing action and requires historical best state', 'probe')
    require(args.work_budgets and all(0 < work <= 1_000_000 for work in args.work_budgets),
            'probe-work', 'work allowances must be in [1, 1000000]', 'probe')
    require(0 < args.seconds <= 300, 'probe-seconds', 'episode deadline must be in (0, 300]', 'probe')
    require(not output.exists(), 'probe-output', 'fresh output directory required', 'probe', output)
    case, selection = result['generation']['case'], protocol['spec']['selection']
    for key in ('request', 'initial'):
        require(sha(case[key]) == result['generation'][key + '_sha256'],
                'probe-input', 'recorded factory input changed', 'probe', case[key])
    baseline_work = selection['route_work']
    budgets = list(dict.fromkeys([baseline_work] + args.work_budgets))
    output.mkdir(parents=True)
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    def open_episode(factory, horizon, work):
        return runtime.open_episode(factory, horizon, args.seconds, work,
                                    result['candidate'], selection['memory'])
    try:
        original = open_episode(case, len(trace), baseline_work)
        for expected in trace[:args.action_number - 1]:
            view = original.observe(False)
            match(original.step(view['handle'], expected['vocabulary_index']), expected)
        before_path = output / 'before.json'
        before_path.write_text(original.export_layout('current'))
        wanted = trace[args.action_number - 1]
        view = original.observe(False)
        baseline = original.step(view['handle'], wanted['vocabulary_index'])
        match(baseline, wanted)
        write(output / 'baseline-replay.json', baseline)
        # Preserve the production request and exact pre-edit geometry. The new
        # episode resets history/reward accounting, which does not drive routing.
        # The original action DTO, catalogs, default repair limits and geometry
        # stay fixed; the baseline trial checks that this isolation is faithful.
        frozen_case = dict(case, initial=str(before_path))
        report = dict(schema='ppo-recorded-action-work-probe-v1', run=str(root),
                      protocol_sha256=sha(root / 'protocol.json'), record=str(record_path),
                      record_sha256=sha(record_path), episode_sha256=sha(episode_path),
                      request_sha256=sha(case['request']), before_sha256=sha(before_path),
                      job=args.job, phase=args.phase, batch=args.batch,
                      episode_index=args.episode_index, action_number=args.action_number,
                      action=wanted['action'], prefix_replayed_exactly=args.action_number - 1,
                      baseline_replayed_exactly=True, baseline_work=baseline_work,
                      repair_limits=runtime.metadata['repair_limits'], trials=[])
        for work in budgets:
            began = time.monotonic()
            episode = open_episode(frozen_case, 1, work)
            view = episode.observe(False)
            actions = json.loads(episode.diagnostic_domain_json())['actions']
            require(wanted['action'] in actions, 'probe-domain',
                    'recorded action absent from frozen geometry domain', 'probe', work)
            step = episode.step(view['handle'], actions.index(wanted['action']))
            if work == baseline_work:
                match(step, baseline, isolated=True)
            write(output / f'work-{work}.json', step)
            metrics = episode.summary()['current_metrics']
            validated = bool(step.get('acceptance', {}).get('candidate_validated'))
            witness = None
            if validated:
                witness_path = output / f'work-{work}-valid.json'
                witness_path.write_text(episode.export_layout('current'))
                independent = open_episode(dict(case, initial=str(witness_path)), 1, work)
                require(independent.summary()['initial_metrics'] == metrics,
                        'probe-validation', 'independent original-request admission differs', 'probe', witness_path)
                witness = dict(path=str(witness_path), sha256=sha(witness_path))
            row = dict(work_budget=work, status=step['status'], validated=validated,
                       metrics=metrics, route_expansions=step['route_expansions'],
                       seconds=time.monotonic() - began, diagnostics=step['diagnostics'],
                       region=step.get('region'), witness=witness)
            report['trials'].append(row)
            write(output / 'report.json', report)
            print(json.dumps({key: value for key, value in row.items() if key != 'region'}), flush=True)
            if validated:
                break
        return report
    finally:
        runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--job', required=True)
    parser.add_argument('--phase', choices=('training', 'evaluation'), required=True)
    parser.add_argument('--batch', type=int, required=True)
    parser.add_argument('--episode-index', type=int, required=True)
    parser.add_argument('--action-number', type=int, required=True)
    parser.add_argument('--work-budgets', type=int, nargs='+', required=True)
    parser.add_argument('--seconds', type=float, default=120)
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
