"""Freeze matched starts from a retained PPO run, reconstructing with its own runtime."""
import argparse
from concurrent.futures import ProcessPoolExecutor, as_completed
import hashlib
import json
import multiprocessing
from pathlib import Path
import shutil
import sys


def read(path):
    return json.loads(Path(path).read_text())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def initialize(native_root, protocol_path):
    # Imports must bind to the baseline's untouched Python and native sources.
    sys.path.insert(0, str(Path(native_root) / 'tools/factory-edit-pilot'))
    from native_factory_runtime import FactoryRuntime
    from ppo_harness_state import check_sources
    global _runtime, _spec
    protocol = read(protocol_path)
    check_sources(protocol)
    _spec = protocol['spec']
    _runtime = FactoryRuntime(_spec['native_library'], protocol['catalog_file'])


def freeze_one(task):
    row, directory = task
    directory = Path(directory)
    directory.mkdir()
    generation = row['generation']
    rebuilt = False
    original_status = generation['status']
    source = generation.get('case')
    if original_status == 'accepted' and not all(Path(source[k]).is_file() for k in ('request', 'initial', 'original')):
        from random_factory_generation import generate
        source, generated = generate(_runtime, _spec, generation['generation_seed'], directory / 'reconstructed', lambda: None)
        if generated['status'] != original_status:
            raise ValueError(f'frozen-start.admission-drift: seed {generation["generation_seed"]}')
        for key in ('request_sha256', 'initial_sha256', 'geometry_sha256'):
            if generated[key] != generation[key]:
                raise ValueError(f'frozen-start.reconstruction-drift: seed {generation["generation_seed"]}, {key}')
        rebuilt = True
    entry = dict(generation_seed=generation['generation_seed'], status=original_status,
                 baseline_result=dict(path=row['result_path'], sha256=row['result_sha256']),
                 reconstructed=rebuilt, generation=generation, files={})
    if source is not None and original_status == 'accepted':
        for key in ('request', 'initial', 'original'):
            path = directory / f'{key}.json'
            shutil.copyfile(source[key], path)
            expected = generation.get(key + '_sha256')
            if expected and sha(path) != expected:
                raise ValueError(f'frozen-start.input-drift: {path}')
            entry['files'][key] = dict(path=str(path.resolve()), sha256=sha(path))
    write(directory / 'entry.json', entry)
    if rebuilt:
        shutil.rmtree(directory / 'reconstructed')
    return entry


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--native-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--batches', type=int, default=32)
    parser.add_argument('--workers', type=int, default=8)
    args = parser.parse_args()
    baseline, output = args.baseline.resolve(), args.output.resolve()
    if args.batches < 1 or not 1 <= args.workers <= 8:
        parser.error('Positive batch count and 1..8 workers are required')
    state, protocol = read(baseline / 'state.json'), read(baseline / 'protocol.json')
    if state['status'] not in ('stopped', 'complete'):
        parser.error('Baseline must be stopped or complete')
    rows = {}
    for job in state['jobs']:
        for reference in job['records']:
            phase = reference['phase']
            if not ((phase['kind'] == 'training' and phase['batch'] <= args.batches)
                    or (phase['kind'] == 'evaluation' and phase['batch'] == 0)):
                continue
            path = baseline / reference['path']
            if sha(path) != reference['sha256']:
                raise ValueError(f'frozen-start.record-hash: {path}')
            for row in read(path)['results']:
                seed = row['generation']['generation_seed']
                if seed in rows and any(rows[seed]['generation'].get(k) != row['generation'].get(k)
                                        for k in ('status', 'request_sha256', 'initial_sha256')):
                    raise ValueError(f'frozen-start.shared-seed-drift: {seed}')
                rows[seed] = row
    expected = {schedule['generation_seed_base'] + seed * 100000 + i
                for seed in protocol['config']['seeds']
                for schedule, count in [(protocol['config']['training'], args.batches * protocol['config']['training']['attempts']),
                                        (protocol['config']['evaluation'], protocol['config']['evaluation']['attempts'])]
                for i in range(count)}
    if set(rows) != expected:
        raise ValueError('frozen-start.incomplete-baseline: required first-batch training/evaluation seeds are absent')
    output.mkdir(parents=True, exist_ok=False)
    tasks = [(row, str(output / str(seed))) for seed, row in sorted(rows.items())]
    entries = []
    with ProcessPoolExecutor(max_workers=args.workers, mp_context=multiprocessing.get_context('spawn'),
                             initializer=initialize, initargs=(str(args.native_root.resolve()), str(baseline / 'protocol.json'))) as pool:
        pending = [pool.submit(freeze_one, task) for task in tasks]
        for future in as_completed(pending):
            entries.append(future.result())
            if len(entries) % 64 == 0:
                print(json.dumps(dict(completed=len(entries), total=len(tasks))), flush=True)
    manifest = dict(schema='ppo-frozen-starts-v1', baseline_protocol=dict(path=str(baseline / 'protocol.json'), sha256=sha(baseline / 'protocol.json')),
                    baseline_state=dict(path=str(baseline / 'state.json'), sha256=sha(baseline / 'state.json')),
                    native_root=str(args.native_root.resolve()), batches=args.batches,
                    entries={str(e['generation_seed']): e for e in sorted(entries, key=lambda e: e['generation_seed'])})
    write(output / 'manifest.json', manifest)
    print(json.dumps(dict(status='complete', cases=len(entries), reconstructed=sum(e['reconstructed'] for e in entries),
                          manifest=str(output / 'manifest.json'), sha256=sha(output / 'manifest.json'))))


if __name__ == '__main__':
    main()
