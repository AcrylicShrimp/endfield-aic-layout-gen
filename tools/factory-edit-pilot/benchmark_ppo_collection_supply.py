"""Replay one frozen batch through transient IPC at explicit CPU worker counts."""
import argparse
from copy import deepcopy
import json
import os
from pathlib import Path
import shutil
import statistics
import time

from ppo_area_update import load_experience
from ppo_harness_pool import Pool
from ppo_harness_state import read, write, sha, checked_path, owned
from ppo_harness_supervisor import process_rows
from profile_ppo_data_supply import fingerprint


def main(args):
    root = args.run.resolve()
    args.output = args.output.resolve()
    if args.output.exists():
        raise ValueError(f'fresh output required: {args.output}')
    args.output.mkdir(parents=True)
    protocol, state = read(root / 'protocol.json'), read(root / 'state.json')
    job = state['jobs'][0]
    reference = next(r for r in job['records'] if r['phase'] == dict(kind='training', batch=args.batch))
    record = read(checked_path(root, reference))
    tasks = [read(Path(r['result_path']).with_name('task.json')) for r in record['results']]
    expected = []
    for result in record['results']:
        payload = load_experience(result) if result['status'] == 'complete' else None
        if payload is not None:
            payload.pop('input_sha256')
        expected.append(fingerprint(payload))
    report = dict(schema='ppo-collection-supply-benchmark-v1', status='running', source_sha256=sha(__file__),
                  source_run=str(root), protocol_sha256=sha(root / 'protocol.json'), record=reference,
                  repetitions=args.repeats, configured_worker_counts=args.workers, cells={},
                  note='Explicit diagnostic pool worker override; retained protocol supplies unchanged task/model inputs.')
    began = time.monotonic()

    def publish():
        write(args.output / 'report.json', report)

    for workers in args.workers:
        destination = args.output / f'workers-{workers}'
        destination.mkdir()
        shutil.copy2(root / 'protocol.json', destination / 'protocol.json')
        checkpoint = checked_path(root, record['behavior_checkpoint'])
        target = owned(destination, record['behavior_checkpoint']['path'])
        target.parent.mkdir(parents=True)
        shutil.copy2(checkpoint, target)
        for task in tasks:
            for reference_file in task.get('initial_start', {}).get('files', {}).values():
                source = checked_path(root, reference_file)
                target = owned(destination, reference_file['path'])
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)
        config = deepcopy(protocol['config'])
        config['execution']['workers'] = workers
        cell = dict(repetitions=[], peak_rss_bytes=0)
        report['cells'][str(workers)] = cell
        last_sample = 0.0

        def progress(**values):
            nonlocal last_sample
            now = time.monotonic()
            if now - began > args.seconds:
                raise TimeoutError(f'collection supply benchmark exceeded {args.seconds} seconds')
            if now - last_sample >= .5:
                rows = process_rows()
                related = {os.getpid()}
                while True:
                    expanded = related | {r[0] for r in rows if r[1] in related}
                    if expanded == related:
                        break
                    related = expanded
                rss = sum(r[3] * 1024 for r in rows if r[0] in related)
                cell['peak_rss_bytes'] = max(cell['peak_rss_bytes'], rss)
                if rss > config['limits']['rss_bytes']:
                    raise MemoryError(f'collection supply process-tree RSS limit exceeded: {rss}')
                last_sample = now

        with Pool(destination, destination / 'pool', config, progress) as pool:
            for repetition in range(args.repeats + 1):
                location = destination / f'repetition-{repetition}'
                paths = []
                for index, original in enumerate(tasks):
                    task = dict(original, retain_details=False)
                    path = location / f'episode-{index:04d}' / 'task.json'
                    path.parent.mkdir(parents=True)
                    write(path, task)
                    paths.append(path)
                results, payloads, timing = pool.collect(paths)
                for index, (result, payload, original) in enumerate(zip(results, payloads, record['results'])):
                    if result['identity'] != original['identity'] or result['status'] != original['status']:
                        raise AssertionError(f'worker count {workers} changed task outcome: {index}')
                    if payload is not None:
                        payload.pop('input_sha256')
                        if result['row']['best_sha256'] != original['row']['best_sha256']:
                            raise AssertionError(f'worker count {workers} changed best geometry: {index}')
                    if fingerprint(payload) != expected[index]:
                        raise AssertionError(f'worker count {workers} changed trajectory/observations: {index}')
                    if 'experience' in result or paths[index].with_name('experience.pt.gz').exists():
                        raise AssertionError('transient collector wrote an experience archive')
                cell['repetitions'].append(dict(repetition=repetition, warmup=repetition == 0, collection=timing,
                                                all_trajectories_exact=True, no_experience_archives=True))
                if repetition:
                    cell['median_collection_seconds'] = statistics.median(
                        r['collection']['seconds'] for r in cell['repetitions'] if not r['warmup'])
                publish()
                print(json.dumps(dict(workers=workers, repetition=repetition, seconds=timing['seconds'],
                                      peak_rss_bytes=cell['peak_rss_bytes'], exact=True)), flush=True)
                del payloads, results
                # Complete measured repetitions are reproducible from the retained
                # source batch. Keep their summary; remove duplicate generated data.
                shutil.rmtree(location)
    report.update(status='complete', seconds=time.monotonic() - began)
    publish()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--batch', type=int, default=3)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--workers', type=int, nargs='+', choices=range(1, 33), default=[8, 12, 16])
    parser.add_argument('--repeats', type=int, choices=range(1, 6), default=3)
    parser.add_argument('--seconds', type=int, choices=range(30, 1801), default=600)
    main(parser.parse_args())
