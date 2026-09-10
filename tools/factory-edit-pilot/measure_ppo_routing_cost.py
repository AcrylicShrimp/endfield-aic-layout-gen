"""Measure frozen routing edits repeatedly, with an optional isolated native variant.

Full-probe time includes episode admission, observation/domain and summary.
Independent candidate validation happens outside timing and is always required.
This selected failure cohort is not an unbiased full-training cost benchmark.
"""
import argparse
import json
import os
from pathlib import Path
import statistics
import time

from native_factory_runtime import FactoryRuntime
from ppo_harness_state import checked_path, check_sources, read, require, sha, write


def run(args):
    root, out = args.run.resolve(), args.output.resolve()
    protocol, sample, matrix = read(root / 'protocol.json'), read(args.sample), read(args.matrix / 'report.json')
    check_sources(protocol)
    require(matrix['sample_sha256'] == sha(args.sample), 'cost-sample', 'matrix sample hash differs')
    record = read(checked_path(root, sample['record']))
    require(not out.exists(), 'cost-output', 'fresh output directory required', entity=out)
    require(1 <= args.repetitions <= 20, 'cost-repetitions', 'repetitions must be in [1, 20]')
    require(args.work_budgets and all(0 < n <= 1_000_000 for n in args.work_budgets),
            'cost-work', 'work allowances must be in [1, 1000000]')
    library = args.native_library or Path(protocol['spec']['native_library'])
    require(0 <= args.preflight_states <= 4096, 'cost-preflight', 'preflight bound must be in [0, 4096]')
    require(not args.preflight_states or args.native_library is not None,
            'cost-variant', 'preflight requires an explicitly supplied isolated library')
    os.environ['AIC_ROUTING_PREFLIGHT_STATES'] = str(args.preflight_states)
    os.environ.pop('AIC_ROUTING_TRACE_DIR', None)
    runtime = FactoryRuntime(str(library.resolve()), protocol['catalog_file'])
    out.mkdir(parents=True)
    report = dict(schema='routing-cost-probe-v1', protocol_sha256=sha(root / 'protocol.json'),
        sample_sha256=sha(args.sample), matrix_sha256=sha(args.matrix / 'report.json'),
        native_library=str(library.resolve()), native_library_sha256=sha(library),
        preflight_states=args.preflight_states, repetitions=args.repetitions, rows=[])
    try:
        for choice, reference in zip(sample['rows'], matrix['rows'], strict=True):
            result = choice['result']
            require(result in record['results'], 'cost-record', 'sample result differs')
            require(choice['episode_index'] == reference['index'], 'cost-index', 'matrix order differs')
            before = args.matrix / f'case-{reference["index"]:02d}-action-{reference["action_number"]:02d}' / 'before.json'
            require(sha(before) == reference['before_sha256'], 'cost-before', 'pre-edit hash differs', entity=before)
            case = dict(result['generation']['case'], initial=str(before.resolve()))
            require(sha(case['request']) == result['generation']['request_sha256'],
                    'cost-request', 'production request hash differs', entity=case['request'])
            selection = protocol['spec']['selection']
            def open_episode(factory, work):
                return runtime.open_episode(factory, 1, 120, work, result['candidate'], selection['memory'])
            rows = {work: dict(work=work, executor_seconds=[], full_probe_seconds=[]) for work in args.work_budgets}
            # Alternate order to reduce systematic drift. One untimed warmup per
            # case/budget, followed by measured repetitions with fresh episodes.
            for repetition in range(-1, args.repetitions):
                budgets = args.work_budgets if repetition % 2 == 0 else list(reversed(args.work_budgets))
                for work in budgets:
                    began = time.perf_counter()
                    episode = open_episode(case, work)
                    view = episode.observe(False)
                    actions = json.loads(episode.diagnostic_domain_json())['actions']
                    require(reference['action'] in actions, 'cost-domain', 'frozen action missing')
                    step = episode.step(view['handle'], actions.index(reference['action']))
                    metrics = episode.summary()['current_metrics']
                    elapsed = time.perf_counter() - began
                    validated = bool(step.get('acceptance', {}).get('candidate_validated'))
                    outcome = dict(validated=validated, status=step['status'], score=metrics['score'],
                        current_sha256=step['current_sha256'], expansions=step['route_expansions'],
                        attempts=len(step.get('region', {}).get('attempts', [])),
                        termination=step.get('region', {}).get('termination'),
                        improves_previous_best_area=validated and metrics['score'][0] < reference['before_best_score'][0])
                    row = rows[work]
                    if 'outcome' not in row:
                        row['outcome'] = outcome
                        write(out / f'case-{reference["index"]:02d}-work-{work}.json', step)
                        if validated:
                            witness = out / f'case-{reference["index"]:02d}-work-{work}-valid.json'
                            witness.write_text(episode.export_layout('current'))
                            admitted = open_episode(dict(case, initial=str(witness)), work)
                            require(admitted.summary()['initial_metrics'] == metrics,
                                    'cost-validation', 'independent admission differs', entity=witness)
                    else:
                        require(row['outcome'] == outcome, 'cost-repeat', 'repeated outcome differs', entity=reference['index'])
                    if not args.native_library:
                        expected = next(t for t in reference['trials'] if t['work'] == work)
                        require(all(outcome[k] == expected[k] for k in ('validated', 'status', 'score', 'expansions')),
                                'cost-reference', 'reference replay differs', entity=reference['index'])
                    if repetition >= 0:
                        row['executor_seconds'].append(step['executor_seconds'])
                        row['full_probe_seconds'].append(elapsed)
            report['rows'].append(dict(index=reference['index'], trials=list(rows.values())))
            write(out / 'report.json', report)
            print(json.dumps(dict(index=reference['index'], results={w: r['outcome']['validated'] for w, r in rows.items()})), flush=True)
        report['summary'] = []
        for work in args.work_budgets:
            trials = [next(t for t in r['trials'] if t['work'] == work) for r in report['rows']]
            summary = dict(work=work, valid=sum(t['outcome']['validated'] for t in trials),
                area_improvements=sum(t['outcome']['improves_previous_best_area'] for t in trials),
                total_expansions=sum(t['outcome']['expansions'] for t in trials))
            for field in ('executor_seconds', 'full_probe_seconds'):
                medians = [statistics.median(t[field]) for t in trials]
                values = sorted(v for t in trials for v in t[field])
                summary[field] = dict(sum_of_case_medians=sum(medians),
                    per_action_median=statistics.median(values), p95=values[int(.95 * (len(values) - 1))])
            report['summary'].append(summary)
        write(out / 'report.json', report)
        print(json.dumps(report['summary'], indent=2))
    finally:
        runtime.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--sample', type=Path, required=True)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--native-library', type=Path)
    parser.add_argument('--preflight-states', type=int, default=0)
    parser.add_argument('--repetitions', type=int, default=5)
    parser.add_argument('--work-budgets', type=int, nargs='+', default=[5000, 10000, 25000, 50000])
    parser.add_argument('--output', type=Path, required=True)
    run(parser.parse_args())
