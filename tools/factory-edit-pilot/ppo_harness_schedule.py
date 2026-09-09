"""Immutable phase tasks, retained-result reuse and post-commit cleanup."""
from collections import Counter
from pathlib import Path
import shutil

from ppo_harness_pool import accept_result
from ppo_harness_state import SCHEMA, require, read, write, owned, sync_directory, mkdir


def phase_tasks(root, protocol, protocol_hash, job, phase):
    config = protocol['config']
    training = phase['kind'] == 'training'
    schedule = config['training' if training else 'evaluation']
    version = job['checkpoint']
    phase_directory = root / job['id'] / f"{phase['kind']}-{phase['batch']:04d}"
    mkdir(phase_directory)
    tasks, cached, attempts = [], {}, []
    for index in range(schedule['attempts']):
        ordinal = (phase['batch'] - 1) * schedule['attempts'] + index if training else index
        generation_seed = schedule['generation_seed_base'] + job['seed'] * 100000 + ordinal
        action_seed = schedule['action_seed_base'] + job['seed'] * 100000 + ordinal
        identity = dict(job=job['id'], phase=phase['kind'], batch=phase['batch'], index=index,
                        generation_seed=generation_seed, action_seed=action_seed)
        logical = dict(schema=SCHEMA, protocol_sha256=protocol_hash, identity=identity, job=job['id'],
                       candidate=job['candidate'], generation_seed=generation_seed, action_seed=action_seed,
                       horizon=config['training']['horizon'], training=training, policy=version['path'],
                       policy_file_sha256=version['sha256'], policy_sha256=version['policy_sha256'],
                       critic_sha256=version['critic_sha256'])
        episode = phase_directory / f'episode-{index:04d}'
        mkdir(episode)
        logical_path = episode / 'logical-task.json'
        if logical_path.exists():
            require(read(logical_path) == logical, 'resumed-task', 'retained task differs from frozen schedule', 'resume', logical_path)
        else:
            write(logical_path, logical)
        prior = sorted(episode.glob('attempt-*'))
        for attempt in prior:
            ready = attempt / 'result-ready.json'
            if ready.is_file():
                require(read(attempt / 'task.json') == logical, 'resumed-task', 'published attempt has a different task', 'resume', ready)
                require(index not in cached, 'duplicate-result', 'multiple published attempts for one task', 'resume', episode)
                cached[index] = accept_result(root, attempt / 'task.json', read(ready), reused=True)
        if index not in cached:
            attempt = episode / f'attempt-{len(prior):04d}'
            mkdir(attempt)
            path = attempt / 'task.json'
            write(path, logical)
            tasks.append((index, path))
            attempts.append(dict(index=index, attempt=len(prior), interrupted_attempts_retained=len(prior)))
    return phase_directory, tasks, cached, attempts


def quality(results):
    complete = [r for r in results if r['status'] == 'complete']
    reductions = [100 * (1 - r['row']['best_metrics']['score'][0] / r['row']['initial_metrics']['score'][0]) for r in complete]
    return dict(attempted=len(results), accepted=len(complete), generation_rejected=len(results) - len(complete),
                selected=sum(r['row']['selected'] for r in complete),
                mean_area_reduction_percent=sum(reductions) / len(reductions) if reductions else None,
                min_area_reduction_percent=min(reductions) if reductions else None,
                max_area_reduction_percent=max(reductions) if reductions else None,
                unique_requests=len({r['generation']['request_sha256'] for r in complete}),
                unique_geometry=len({r['generation']['geometry_sha256'] for r in complete}),
                distinct_rates=len({(r['generation']['draw']['product'], r['generation']['draw']['rate']['duration_ms']) for r in complete}),
                facility_counts=sorted({r['row']['instances'] for r in complete}),
                products=dict(Counter(r['generation']['draw']['product'] for r in complete)),
                total_generation_seconds=sum(r['generation']['seconds'] for r in results),
                total_episode_seconds=sum(r['row']['seconds'] for r in complete))


def evaluation_pairs(results, baseline):
    by_seed = lambda rows: {r['identity']['generation_seed']: r for r in rows if r['status'] == 'complete'}
    current, initial = by_seed(results), by_seed(baseline)
    shared = sorted(current.keys() & initial.keys())
    differences = []
    for seed in shared:
        first, last = initial[seed], current[seed]
        require(all(first['generation'][key] == last['generation'][key] for key in ('request_sha256', 'initial_sha256')),
                'evaluation-reset-drift', 'matched evaluation factory changed', 'evaluation', seed)
        start_area = first['row']['initial_metrics']['score'][0]
        differences.append(100 * (first['row']['best_metrics']['score'][0] - last['row']['best_metrics']['score'][0]) / start_area)
    return dict(shared_seeds=shared, baseline_accepted=len(initial), current_accepted=len(current),
                paired_delta_pp=sum(differences) / len(differences) if differences else None,
                wins=sum(value > 0 for value in differences), ties=sum(value == 0 for value in differences),
                losses=sum(value < 0 for value in differences))


def cleanup_phase(root, phase_directory, results, retain_training):
    """Called only after the state pointer commits this phase's record."""
    removed = []
    for result in results:
        attempt = owned(root, result['result_path']).parent
        require(attempt.is_relative_to(phase_directory.resolve()), 'cleanup-path', 'result outside committed phase', 'retention', attempt)
        task = read(attempt / 'task.json')
        if task['training'] and not retain_training:
            paths = [attempt / 'generated', attempt / 'episode', attempt / 'experience.pt.gz']
        else:
            location = owned(root, result['generation']['original_attempt']['directory'])
            require(location.is_relative_to(attempt), 'cleanup-path', 'constructor outside result attempt', 'retention', location)
            paths = [location / 'construction.html']
            if result['status'] == 'complete':
                paths.append(location / 'construction.json')
        for path in paths:
            require(not path.is_symlink(), 'cleanup-symlink', 'refusing a generated symlink', 'retention', path)
            path = owned(root, path)
            require(path.is_relative_to(attempt) and path != attempt, 'cleanup-path',
                    'generated cleanup path leaves its result attempt', 'retention', path)
            if path.exists():
                removed.append(str(path.relative_to(root)))
                shutil.rmtree(path) if path.is_dir() else path.unlink()
                sync_directory(path.parent)
    write(phase_directory / 'retention.json', dict(status='complete', removed=removed,
                                                retained_training_experience=retain_training))
