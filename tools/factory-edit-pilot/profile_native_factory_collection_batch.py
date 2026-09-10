"""Native worker-count replay with explicit checkpoint import and retained-worker repetitions."""
import argparse
import os
from pathlib import Path
import shutil
import time

import torch

from native_factory_runtime import FactoryRuntime
from profile_native_factory_ownership import current_spec
from ppo_harness_protocol import sources, environment
from ppo_harness_state import SCHEMA

from direct_area_audit import physics_signature
from full_factory_screen import sha
from ppo_harness_pool import Pool
from ppo_harness_schedule import phase_tasks, quality, cleanup_phase
from ppo_harness_state import read, write
from ppo_harness_supervisor import process_rows


def run(args):
    if args.output.exists():
        raise ValueError(f'ownership-batch.output-exists: {args.output}')
    args.output.mkdir(parents=True)
    protocol = read(args.screen / 'protocol.json')
    protocol.update(schema=SCHEMA, backend='retained-full-factory-native', spec=current_spec(protocol), environment=environment())
    if args.workers is not None:
        protocol['config']['execution']['workers'] = args.workers
    catalog_file = args.output / 'catalogs.json'
    write(catalog_file, protocol['spec']['catalogs'])
    protocol['catalog_file'] = str(catalog_file)
    runtime = FactoryRuntime(protocol['spec']['native_library'], catalog_file)
    from native_factory_runtime import ROOT
    paths = sources() | {ROOT / p for p in runtime.build['files_sha256']}
    paths.update(Path(protocol['spec'][k]) for k in ('native_library', 'cli'))
    paths.update(map(Path, protocol['spec']['catalogs'].values()))
    paths.update(Path(p['source']) for p in protocol['spec']['products'])
    paths.add(catalog_file)
    protocol['sources'] = {str(p): sha(p) for p in sorted(paths)}
    write(args.output / 'protocol.json', protocol)
    runtime_metadata = runtime.metadata
    runtime.close()
    reference = read(next((args.screen/'current-history-16--seed-17'/'training-0004').glob('record-*.json')))
    checkpoint = dict(reference['behavior_checkpoint'])
    original = args.screen / checkpoint['path']
    if sha(original) != checkpoint['sha256']:
        raise ValueError('ownership-batch.reference-checkpoint-hash')
    saved = torch.load(original, map_location='cpu', weights_only=False)
    saved.update(schema=SCHEMA, protocol_sha256=sha(args.output / 'protocol.json'),
                 native_replay_import=dict(source=str(original), sha256=sha(original), purpose='read-only ownership comparison'))
    target = args.output / checkpoint['path']
    target.parent.mkdir(parents=True)
    torch.save(saved, target)
    checkpoint['sha256'] = sha(target)
    job = dict(id=reference['job'], candidate=reference['candidate'], seed=reference['seed'], checkpoint=checkpoint)
    phase_directory, tasks, cached, attempts = phase_tasks(args.output, protocol, sha(args.output/'protocol.json'), job, reference['phase'])
    if cached:
        raise ValueError('ownership-batch.unexpected-reused-task')
    began, last = time.monotonic(), 0.
    report = dict(schema='native-full-factory-ownership-batch-v2', status='running', source_sha256=sha(__file__),
                  reference_protocol_sha256=sha(args.screen/'protocol.json'), behavior_checkpoint=checkpoint,
                  reference_checkpoint=reference['behavior_checkpoint'], runtime=runtime_metadata,
                  peak_rss_bytes=0, max_processes=0, results=[], repetitions=[], workers=protocol['config']['execution']['workers'])
    last_stage = None
    def progress(**value):
        nonlocal last, last_stage
        now = time.monotonic()
        if now-began > args.seconds:
            raise TimeoutError(f'ownership-batch.budget: {args.seconds} seconds')
        if now-last >= .25:
            rows = process_rows()
            related = {os.getpid()}
            while True:
                expanded = related | {r[0] for r in rows if r[1] in related}
                if expanded == related:
                    break
                related = expanded
            processes = [r for r in rows if r[0] in related]
            rss = sum(r[3]*1024 for r in processes)
            report['peak_rss_bytes'] = max(report['peak_rss_bytes'], rss)
            report['max_processes'] = max(report['max_processes'], len(processes))
            if rss > protocol['config']['limits']['rss_bytes']:
                raise MemoryError('ownership-batch.aggregate-rss-limit')
            last = now
        if value.get('advanced') or value['stage'] != last_stage:
            print(dict(**value, elapsed_seconds=now-began), flush=True)
            last_stage = value['stage']
    try:
        expected_signatures = [physics_signature(r) for r in reference['results']]
        logical_tasks = [read(p) for _, p in tasks]
        with Pool(args.output, args.output/'workers', protocol['config'], progress) as pool:
            for repetition in range(args.repeats):
                if repetition:
                    directory = args.output / f'repetition-{repetition}'
                    paths = []
                    for index, logical in enumerate(logical_tasks):
                        path = directory / f'episode-{index:04d}' / 'task.json'
                        path.parent.mkdir(parents=True)
                        write(path, logical)
                        paths.append(path)
                else:
                    paths = [p for _, p in tasks]
                results, timing = pool.collect(paths)
                signatures = [physics_signature(r) for r in results]
                if signatures != expected_signatures or any(r['identity'] != e['identity'] for r, e in zip(results, reference['results'])):
                    raise ValueError(f'ownership-batch.replay-drift: repetition {repetition}')
                report['repetitions'].append(dict(repetition=repetition, cold=repetition == 0,
                    collection=timing, quality=quality(results), semantic_sha256=signatures,
                    all_generation_physical_and_sampling_parity=True))
                if not repetition:
                    report.update(results=results, collection=timing, quality=quality(results))
                write(args.output/'report.json', report)
                if repetition:
                    # Identical regenerable repeats are represented by the first
                    # retained inputs/experiences plus verified semantic hashes.
                    shutil.rmtree(directory)
                print(dict(stage='repetition-complete', workers=report['workers'], repetition=repetition,
                           seconds=timing['seconds'], peak_rss_bytes=report['peak_rss_bytes']), flush=True)
        report['elapsed_seconds'] = time.monotonic()-began
        report.update(status='complete', all_generation_physical_and_sampling_parity=True)
        write(args.output/'report.json', report)
        # Preserve selected source inputs, experiences, checkpoints and final witnesses.
        cleanup_phase(args.output, phase_directory, report['results'], retain_training=True)
        print({k:v for k,v in report.items() if k not in ('results','repetitions','behavior_checkpoint','reference_checkpoint','runtime')}, flush=True)
    except BaseException as error:
        report.update(status='failed', diagnostic=dict(code='ownership-batch.failed', severity='error',
                      stage='collection', entity=str(args.output), message=str(error)))
        write(args.output/'report.json', report)
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=600)
    parser.add_argument('--workers', type=int)
    parser.add_argument('--repeats', type=int, default=1)
    args = parser.parse_args()
    if not 0 < args.seconds <= 1200:
        parser.error('seconds must be in (0, 1200]')
    if args.workers is not None and not 1 <= args.workers <= 32 or not 1 <= args.repeats <= 8:
        parser.error('workers must be 1..32 and repeats must be 1..8')
    run(args)
