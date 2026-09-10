"""Native 24-worker replay with explicit checkpoint import and current runtime provenance."""
import argparse
import os
from pathlib import Path
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
    report = dict(schema='native-full-factory-ownership-batch-v1', status='running', source_sha256=sha(__file__),
                  reference_protocol_sha256=sha(args.screen/'protocol.json'), behavior_checkpoint=checkpoint,
                  reference_checkpoint=reference['behavior_checkpoint'], runtime=runtime_metadata,
                  peak_rss_bytes=0, max_processes=0, results=[], workers=protocol['config']['execution']['workers'])
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
        with Pool(args.output, args.output/'workers', protocol['config'], progress) as pool:
            results, timing = pool.collect([p for _, p in tasks])
        report.update(results=results, collection=timing, elapsed_seconds=time.monotonic()-began,
                      quality=quality(results))
        for reference_result, result in zip(reference['results'], results):
            if result['identity'] != reference_result['identity'] or physics_signature(result) != physics_signature(reference_result):
                raise ValueError(f'ownership-batch.replay-drift: {result["identity"]}')
        report.update(status='complete', all_generation_physical_and_sampling_parity=True)
        write(args.output/'report.json', report)
        # Preserve selected source inputs, experiences, checkpoints and final witnesses.
        cleanup_phase(args.output, phase_directory, results, retain_training=True)
        print({k:v for k,v in report.items() if k not in ('results','behavior_checkpoint','reference_checkpoint','runtime')}, flush=True)
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
    args = parser.parse_args()
    if not 0 < args.seconds <= 1200:
        parser.error('seconds must be in (0, 1200]')
    run(args)
