"""Run the bounded original-versus-mask screen with frozen baseline checkpoints."""
import argparse
from collections import Counter
from copy import deepcopy
import os
from pathlib import Path
import statistics
import sys
import time

SOURCE = Path(os.environ.get('AIC_MASK_SOURCE_ROOT', Path(__file__).resolve().parents[3]))
sys.path[:0] = [str(SOURCE / 'tools/factory-edit-pilot'), str(SOURCE / 'tools/factory-edit-pilot/operations')]

import torch
from blueprint_execution_pool import BlueprintPool
from blueprint_execution_resources import ResourceGuard, cpu_allocation
from blueprint_ppo_store import checked, load, read, sha, write
from blueprint_ppo_update import digest, require
from declarative_learning_launch import evaluation_cases, sources
from declarative_learning_policy import ActorCritic
from declarative_quality_analysis import aggregate, geometry_outcomes, paired
from band_mask_policy import CODES


def bound(path):
    return dict(path=str(Path(path).resolve()), sha256=sha(path))


def behavior(rows):
    counts, diagnostics = Counter(), Counter()
    for row in rows:
        counts.update(row['counts']); diagnostics.update(row['band_mask']['diagnostics'])
    return dict(episodes=len(rows), mean_counts={k: v / len(rows) for k, v in counts.items()},
        diagnostics=dict(diagnostics), impossible_band_actions=sum(diagnostics[k] for k in CODES),
        unfinished=sum(r['unfinished_draft'] for r in rows), guards=dict(Counter(r['guard'] for r in rows)),
        mean_seconds={k: statistics.mean(r[k] for r in rows) for k in ('admission_seconds', 'inference_seconds')},
        mean_mask_seconds={k: statistics.mean(r['band_mask'][k] for r in rows)
            for k in ('prepare_seconds', 'mask_seconds', 'mask_apply_seconds')})


def summarize(blocks, panel, strata, replicates):
    outcomes = {variant: [] for variant in ('original', 'masked')}
    by_seed = {}
    for seed in (71, 113):
        one = {}
        for variant in outcomes:
            block = next(b for b in blocks if b['seed'] == seed and b['variant'] == variant)
            rows = block['rows']
            require(all(r['seed'] == seed and r['update'] == 128 and r['policy_actions'] == 64
                and r['band_mask']['enabled'] == (variant == 'masked') for r in rows),
                'band-mask.evaluation: changed seed/checkpoint/action budget/variant')
            value = geometry_outcomes(rows, panel, strata, replicates=replicates)
            outcomes[variant].append(value)
            one[variant] = aggregate([value], strata)
        by_seed[str(seed)] = paired(one['masked'], one['original'], strata)
        by_seed[str(seed)]['scope'] = 'Paired geometries conditional on this frozen model and four action draws.'
    totals = {v: aggregate(values, strata) for v, values in outcomes.items()}
    comparison = paired(totals['masked'], totals['original'], strata)
    comparison['scope'] = ('Paired geometries, stratified by size, with both frozen training seeds and all action '
        'replicates attached. Exploratory reuse of the earlier confirmation panel; no training-effect claim.')
    return dict(quality=totals, comparison=comparison, by_seed=by_seed,
        behavior={v: behavior([r for b in blocks if b['variant'] == v for r in b['rows']]) for v in outcomes})


def run(args):
    output = args.output.resolve()
    require(not output.exists(), f'band-mask.output: fresh directory required: {output}')
    require(args.replicates == 4 and args.workers == 16 and 0 < args.seconds <= 1200,
        'band-mask.protocol: four action replicates, sixteen workers, at most twenty minutes')
    panel = read(args.panel)
    require(panel['status'] == 'complete' and len(panel['cases']) == 32 and
        Counter(c['stratum'] for c in panel['cases']) == {'small': 16, 'large': 16},
        'band-mask.panel: completed balanced 32-geometry cohort required')
    output.mkdir(parents=True)
    torch.set_num_threads(1); torch.set_num_interop_threads(1); torch.use_deterministic_algorithms(True)
    selected = {}
    for seed, branch in ((71, args.branch71), (113, args.branch113)):
        launch_path = branch / 'launch.json'; launch = read(launch_path)
        owner = branch / f'seed-{seed}'; job = read(owner / 'state.json')
        require(job['update'] == 128 and job['status'] == 'complete', 'band-mask.checkpoint: completed endpoint required')
        checkpoint = checked(job['latest_checkpoint'], owner)
        saved = load(checkpoint)
        require(saved['seed'] == seed and saved['update'] == 128 and
            saved['protocol_sha256'] == sha(launch_path) and saved['model_sha256'] == digest(saved['model']),
            f'band-mask.checkpoint: changed model binding: {checkpoint}')
        require(launch['config']['success'].get('sampling') is None,
            'band-mask.reference: baseline-8 checkpoints required')
        selected[seed] = dict(launch=launch, launch_binding=bound(launch_path), checkpoint=bound(checkpoint),
            model_sha256=saved['model_sha256'], weights=saved['model'])
        del saved
    original = selected[71]['launch']['config']
    require(original['actions'] == selected[113]['launch']['config']['actions'] and
        original['native'] == selected[113]['launch']['config']['native'] and
        original['ppo'] == selected[113]['launch']['config']['ppo'] and
        original['evaluation'] == selected[113]['launch']['config']['evaluation'],
        'band-mask.reference: source models require identical action/native/model/evaluation settings')
    library = selected[71]['launch']['source']['native_library']
    require(sha(library) == selected[71]['launch']['source']['native_sha256'] ==
        selected[113]['launch']['source']['native_sha256'], 'band-mask.native: source models require identical native binary')
    for seed in selected:
        reserved = selected[seed]['launch']['config']['training_input']['geometry_reservation']['geometry_sha256']
        require(all(c['geometry_identity'] in reserved for c in panel['cases']),
            'band-mask.panel: evaluation geometries must have been reserved from both source training runs')
    config = deepcopy(original)
    # Evaluation has no logical training streams. Retain the same physical pool,
    # worker-local models, case cache, native evaluator and shared-memory protocol.
    config['training']['streams'] = args.workers
    config['evaluation']['replicates'] = args.replicates
    allocation = cpu_allocation(args.workers)
    source = sources(original, library)
    scripts = [bound(Path(__file__).with_name(name)) for name in
        ('band_mask_policy.py', 'band_mask_worker.py', 'band_mask_experiment.py', 'test_band_mask_policy.py')]
    scripts.append(bound(Path(__file__).resolve().parents[1] / 'declarative_band_mask.py'))
    protocol = dict(schema='band-mask-frozen-screen-v1', status='registered', panel=bound(args.panel),
        models={str(s): {k: v for k, v in x.items() if k not in ('weights', 'launch')} for s, x in selected.items()},
        source=source, scripts=scripts, evaluation=config['evaluation'], allocation=allocation,
        max_seconds=args.seconds, max_output_bytes=2 * 1024**3, max_rss_bytes=32 * 1024**3,
        training_updates=0, normalization='original candidate counts; mask logits without reindexing',
        order=[[71, 'original'], [71, 'masked'], [113, 'masked'], [113, 'original']])
    write(output / 'protocol.json', protocol)
    guard = ResourceGuard(output, args.seconds, protocol['max_rss_bytes'], 16 * 1024**3)
    blocks = []
    try:
        with BlueprintPool(config, library, allocation['workers'], output / 'workers', guard,
                worker_entry=Path(__file__).with_name('band_mask_worker.py')) as workers:
            os.sched_setaffinity(0, allocation['coordinator'])
            protocol['workers'] = workers.identities
            model = ActorCritic(config['ppo']['width'], config['ppo']['message_rounds']).eval()
            cases = evaluation_cases(panel, config)
            # Warm each persistent worker once per condition; excluded from outcomes.
            warm = [c for s in ('small', 'large') for c in panel['cases'] if c['stratum'] == s]
            warm = warm[:8] + warm[16:24]
            model.load_state_dict(selected[71]['weights'], strict=True)
            for variant in ('original', 'masked'):
                path = output / 'warmup' / variant; path.mkdir(parents=True)
                rows = workers.evaluate(model, [dict(c, sample=0, mask_variant=variant) for c in warm],
                    71, 128, path, guard.began + guard.seconds)
                write(path / 'report.json', dict(rows=rows))
                for row in rows:
                    (path / f'factory-{row["generation_seed"]}-{row["sample"]}.pt.gz').unlink()
            owned_bytes = 0
            for seed, variant in protocol['order']:
                guard(); model.load_state_dict(selected[seed]['weights'], strict=True)
                path = output / f'seed-{seed}' / variant; path.mkdir(parents=True)
                began = time.monotonic(); cpu_before = sum(workers.worker_cpu_seconds)
                rows = workers.evaluate(model, [dict(c, mask_variant=variant) for c in cases], seed, 128,
                    path, guard.began + guard.seconds)
                block = dict(seed=seed, variant=variant, rows=rows, seconds=time.monotonic() - began,
                    worker_cpu_seconds=sum(workers.worker_cpu_seconds) - cpu_before)
                write(path / 'report.json', block)
                blocks.append(block)
                owned_bytes += sum((path / f'factory-{r["generation_seed"]}-{r["sample"]}.pt.gz').stat().st_size for r in rows)
                require(owned_bytes < protocol['max_output_bytes'], 'band-mask.resource: retained output ceiling reached')
                require(digest(model.state_dict()) == selected[seed]['model_sha256'] and
                    all(r['model_sha256'] == selected[seed]['model_sha256'] for r in
                        workers.exchange([dict(operation='model_digest') for _ in workers.workers])),
                    'band-mask.frozen-model: evaluation changed model weights')
                write(output / 'progress.json', dict(status='running', completed=[dict(seed=b['seed'], variant=b['variant'],
                    seconds=b['seconds'], episodes=len(b['rows'])) for b in blocks], retained_snapshot_bytes=owned_bytes))
                print(dict(stage='evaluated', seed=seed, variant=variant, episodes=len(rows), seconds=block['seconds'],
                    mean_area_reduction=statistics.mean(r['total_area_reduction_percent'] for r in rows)), flush=True)
        for x in selected.values():
            require(sha(x['checkpoint']['path']) == x['checkpoint']['sha256'], 'band-mask.checkpoint: source file changed')
        result = dict(schema='band-mask-frozen-screen-result-v1', status='complete',
            protocol=bound(output / 'protocol.json'), training_updates=0, seconds=time.monotonic() - guard.began,
            retained_snapshot_bytes=owned_bytes, resource=guard.last, workers=protocol['workers'],
            blocks=[dict(seed=b['seed'], variant=b['variant'], seconds=b['seconds'], worker_cpu_seconds=b['worker_cpu_seconds'],
                report=bound(output / f'seed-{b["seed"]}' / b['variant'] / 'report.json')) for b in blocks],
            **summarize(blocks, panel, config['strata'], args.replicates))
        write(output / 'result.json', result)
        print(dict(status='complete', comparison=result['comparison'], seconds=result['seconds']), flush=True)
        return result
    except BaseException as error:
        write(output / 'failure.json', dict(status='failed', error=str(error),
            completed=[dict(seed=b['seed'], variant=b['variant']) for b in blocks]))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--branch71', type=Path, required=True)
    parser.add_argument('--branch113', type=Path, required=True)
    parser.add_argument('--panel', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--workers', type=int, default=16)
    parser.add_argument('--replicates', type=int, default=4)
    parser.add_argument('--seconds', type=float, default=1200)
    run(parser.parse_args())
