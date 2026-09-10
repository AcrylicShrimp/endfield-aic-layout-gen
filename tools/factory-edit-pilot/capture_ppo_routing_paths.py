"""Capture exact path inputs using a separately built diagnostic native library.

Apply routing_path_capture.patch only in an isolated source checkout. Supply an
existing probe_ppo_recorded_action output; every captured action must reproduce
its saved result. Input dumping changes elapsed times, so do not benchmark it.
"""
import argparse
import json
import os
from pathlib import Path

from native_factory_runtime import FactoryRuntime
from ppo_harness_state import read, require, sha, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--probe', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out, source = args.output.resolve(), args.probe.resolve()
    report, protocol = read(source / 'report.json'), read(args.run / 'protocol.json')
    require(sha(args.run / 'protocol.json') == report['protocol_sha256'],
            'capture-protocol', 'probe protocol hash differs', entity=args.run)
    require(sha(report['record']) == report['record_sha256'],
            'capture-record', 'record hash differs', entity=report['record'])
    require(sha(source / 'before.json') == report['before_sha256'],
            'capture-before', 'pre-edit geometry hash differs', entity=source)
    results = [r for r in read(report['record'])['results']
               if r['identity']['index'] == report['episode_index']]
    require(len(results) == 1, 'capture-result', 'expected one matching episode')
    require(not out.exists(), 'capture-output', 'fresh output directory required', entity=out)
    case = dict(results[0]['generation']['case'], initial=str(source / 'before.json'))
    require(sha(case['request']) == report['request_sha256'],
            'capture-request', 'request hash differs', entity=case['request'])
    runtime = FactoryRuntime(str(args.native_library.resolve()), protocol['catalog_file'])
    out.mkdir(parents=True)
    old_directory = os.environ.get('AIC_ROUTING_TRACE_DIR')
    try:
        for trial in report['trials']:
            work = trial['work_budget']
            directory = out / f'work-{work}'
            directory.mkdir()
            os.environ['AIC_ROUTING_TRACE_DIR'] = str(directory)
            episode = runtime.open_episode(case, 1, 120, work, results[0]['candidate'],
                                           protocol['spec']['selection']['memory'])
            view = episode.observe(False)
            actions = json.loads(episode.diagnostic_domain_json())['actions']
            step = episode.step(view['handle'], actions.index(report['action']))
            baseline = read(source / f'work-{work}.json')
            for key in ('action', 'status', 'executor_status', 'route_expansions', 'current_sha256', 'best_sha256'):
                require(step[key] == baseline[key], 'capture-replay', f'result differs: {key}')
            paths = sorted(directory.glob('path-*.json'))
            require(paths, 'capture-instrumentation', 'library did not emit path inputs', entity=args.native_library)
            write(directory / 'result.json', step)
            print(json.dumps(dict(work=work, paths=len(paths), expansions=step['route_expansions'])))
    finally:
        if old_directory is None:
            os.environ.pop('AIC_ROUTING_TRACE_DIR', None)
        else:
            os.environ['AIC_ROUTING_TRACE_DIR'] = old_directory
        runtime.close()


if __name__ == '__main__':
    main()
