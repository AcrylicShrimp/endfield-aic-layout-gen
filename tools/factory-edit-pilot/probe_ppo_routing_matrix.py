"""Replay a frozen census sample at every requested routing work allowance."""
import argparse
import gzip
import json
from pathlib import Path
import time

from native_factory_runtime import FactoryRuntime
from ppo_harness_state import checked_path, check_sources, read, require, sha, write
from probe_ppo_recorded_action import match


def run(args):
    root, out = args.run.resolve(), args.output.resolve()
    protocol, sample = read(root / 'protocol.json'), read(args.sample)
    check_sources(protocol)
    record = read(checked_path(root, sample['record']))
    require(not out.exists(), 'matrix-output', 'fresh output directory required', entity=out)
    require(args.work_budgets and all(0 < n <= 1_000_000 for n in args.work_budgets),
            'matrix-work', 'work allowances must be in [1, 1000000]')
    selection = protocol['spec']['selection']
    baseline_work = selection['route_work']
    budgets = list(dict.fromkeys([baseline_work] + args.work_budgets))
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    out.mkdir(parents=True)
    rows = []
    try:
        for choice in sample['rows']:
            result = choice['result']
            require(result in record['results'], 'matrix-sample', 'sample result differs from record')
            case = result['generation']['case']
            for key in ('request', 'initial'):
                require(sha(case[key]) == result['generation'][key + '_sha256'],
                        'matrix-input', 'factory input hash differs', entity=case[key])
            with gzip.open(Path(result['row']['directory']) / 'episode.json.gz', 'rt') as stream:
                trace = json.load(stream)['trace']
            target = choice['selected_action_number']
            require(1 <= target <= len(trace), 'matrix-action', 'action outside trajectory')
            wanted = trace[target - 1]
            require(wanted['action']['kind'] != 'restore_best', 'matrix-action', 'expected routing edit')
            directory = out / f'case-{choice["episode_index"]:02d}-action-{target:02d}'
            directory.mkdir()

            def open_episode(factory, horizon, work):
                return runtime.open_episode(factory, horizon, 120, work,
                                            result['candidate'], selection['memory'])

            original = open_episode(case, len(trace), baseline_work)
            for expected in trace[:target - 1]:
                view = original.observe(False)
                match(original.step(view['handle'], expected['vocabulary_index']), expected)
            before = directory / 'before.json'
            before.write_text(original.export_layout('current'))
            view = original.observe(False)
            match(original.step(view['handle'], wanted['vocabulary_index']), wanted)
            item = dict(index=choice['episode_index'], product=choice['product'], instances=choice['instances'],
                action_number=target, action=wanted['action'],
                before_current_score=trace[target - 2]['current_score'] if target > 1 else result['row']['initial_metrics']['score'],
                before_best_score=trace[target - 2]['best_score'] if target > 1 else result['row']['initial_metrics']['score'],
                before_sha256=sha(before), trials=[])
            frozen = dict(case, initial=str(before))
            for work in budgets:
                began = time.monotonic()
                episode = open_episode(frozen, 1, work)
                view = episode.observe(False)
                actions = json.loads(episode.diagnostic_domain_json())['actions']
                require(wanted['action'] in actions, 'matrix-domain', 'recorded action missing')
                step = episode.step(view['handle'], actions.index(wanted['action']))
                if work == baseline_work:
                    match(step, wanted, isolated=True)
                metrics = episode.summary()['current_metrics']
                valid = bool(step.get('acceptance', {}).get('candidate_validated'))
                if valid:
                    witness = directory / f'work-{work}-valid.json'
                    witness.write_text(episode.export_layout('current'))
                    admitted = open_episode(dict(case, initial=str(witness)), 1, work)
                    require(admitted.summary()['initial_metrics'] == metrics,
                            'matrix-validation', 'independent original-request admission differs', entity=witness)
                write(directory / f'work-{work}.json', step)
                item['trials'].append(dict(work=work, validated=valid, score=metrics['score'],
                    executor_seconds=step['executor_seconds'], probe_seconds=time.monotonic() - began,
                    expansions=step['route_expansions'], status=step['status'], diagnostics=step['diagnostics'],
                    region=step.get('region'),
                    improves_previous_best_area=valid and metrics['score'][0] < item['before_best_score'][0]))
            rows.append(item)
            write(out / 'report.json', dict(schema='route-work-budget-matrix-v1',
                sample_sha256=sha(args.sample), selection='frozen census sample', rows=rows))
            print(json.dumps(dict(index=item['index'], valid=[t['validated'] for t in item['trials']])), flush=True)
    finally:
        runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--sample', type=Path, required=True)
    parser.add_argument('--work-budgets', type=int, nargs='+', default=[5000, 10000, 25000, 50000])
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
