"""Prepare, run and resume duration-bounded blueprint success replay."""
import argparse
from datetime import datetime, timezone
from pathlib import Path
import signal
import time

import torch

from blueprint_duration_store import SequenceCache, boundary_stop, exclusive_run, prune_checkpoints, save_compact, storage_guard
from blueprint_ppo import check_sources
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_evaluation_cohort import evaluate_cohort, load_cohort
from blueprint_ppo_store import ROOT, checked, load, read, reference, restore_learners, sha, write
from blueprint_ppo_update import digest, learners, require
from blueprint_success import initialize as initialize_success, restore_buffer, snapshot
from blueprint_success_buffer import Evidence, SuccessBuffer
from blueprint_success_update import update_success
from blueprint_duration_metrics import training_quality
from ppo_harness_environment import load_environment, tracking_destination


def prepare(config_path, library, output, readiness=False, env_file=None, evaluation_cohort=None):
    raw = read(config_path)
    require(raw['duration']['schema'] == 'blueprint-success-duration-v2',
            'blueprint.duration: v2 configuration required; keep historical v1 runs with their frozen runner')
    require(readiness or raw.get('tracking', {}).get('backend') == 'wandb'
            and raw['tracking'].get('mode') == 'online', 'blueprint.duration: normal jobs require online W&B')
    if not readiness and raw['tracking']['backend'] == 'wandb':
        load_environment(env_file or ROOT / '.env')
        destination = tracking_destination(required=True)
    initialize_success(config_path, library, output, 'main', .05)
    protocol = read(output / 'protocol.json')
    config = protocol['config']
    require(config['duration']['schema'] == 'blueprint-success-duration-v2', 'blueprint.duration: duration schema')
    require(len(config['seeds']) == 1, 'blueprint.duration: one continuing learner')
    require(config['training']['episode_actions'] == 64 and config['training']['chunk_actions'] == 32,
            'blueprint.duration: 32-action chunks and 64-action true episodes')
    if readiness:
        config['purpose'] = 'blueprint-success-duration-readiness'
        config['training']['updates'] = 6
        config['duration'].update(seconds=120, min_free_bytes=0)
        config['tracking'] = dict(backend='disabled')
        config['limits'].update(run_seconds=300, max_output_bytes=1024**3)
    elif config['tracking']['backend'] == 'wandb':
        config['tracking'].update(destination)
    require(config['duration']['seconds'] > 0 and config['duration']['finish_grace_seconds'] > 0
            and config['evaluation']['interval_batches'] == 4
            and 'evaluation_interval_seconds' not in config['duration'], 'blueprint.duration: time bounds and four-batch evaluation')
    require(config['limits']['run_seconds'] == config['duration']['seconds'] + config['duration']['finish_grace_seconds'],
            'blueprint.duration: explicit hard finalization deadline')
    protocol['schema'] = 'blueprint-success-duration-v2'
    protocol['source_root'] = str(ROOT)
    for name in ('blueprint_duration.py', 'blueprint_duration_store.py', 'blueprint_duration_job.py', 'audit_blueprint_duration.py',
                 'audit_blueprint_success.py', 'audit_blueprint_ppo.py', 'blueprint_duration_tracking.py',
                 'blueprint_duration_metrics.py', 'blueprint_evaluation_cohort.py', 'blueprint_duration_tracking_job.py',
                 'ppo_harness_environment.py', 'ppo_harness_state.py'):
        path = Path(__file__).with_name(name)
        protocol['sources'][str(path.relative_to(ROOT))] = sha(path)
    spec = read(ROOT / config['probe_spec'])
    cohort_path = (evaluation_cohort or ROOT / config['evaluation']['cohort']).resolve()
    config['evaluation']['cohort'] = str(cohort_path)
    _, protocol['external_sources'] = load_cohort(cohort_path, config['evaluation']['expected_cases'],
                                                 {k: ROOT / p for k, p in spec['catalogs'].items()})
    write(output / 'protocol.json', protocol)
    write(output / 'state.json', dict(schema='blueprint-success-duration-v2', status='prepared',
        protocol_sha256=sha(output / 'protocol.json'), seconds=0., jobs={}, stop_reason=None,
        requested_seconds=config['duration']['seconds'], prepared_at=datetime.now(timezone.utc).isoformat()))


def run(output, stop_after_updates=None, env_file=None):
    with exclusive_run(output):
        if stop_after_updates is None:
            from blueprint_duration_job import execute
            return execute(output, env_file)
        return run_locked(output, stop_after_updates)


def run_locked(output, stop_after_updates=None, health=None):
    protocol, state = read(output / 'protocol.json'), read(output / 'state.json')
    require(protocol['schema'] == state['schema'] == 'blueprint-success-duration-v2',
            'blueprint.duration: v2 required; historical v1 resume requires its frozen runner')
    require(state['protocol_sha256'] == sha(output / 'protocol.json'), 'blueprint.duration: protocol binding')
    require(state['status'] != 'complete', 'blueprint.duration: already complete')
    check_sources(protocol)
    config, seed = protocol['config'], protocol['config']['seeds'][0]
    require(stop_after_updates is None or config['purpose'] == 'blueprint-success-duration-readiness',
            'blueprint.duration: stop-after-updates is a declared readiness facility')
    runtime = Runtime(config, protocol['native_library'])
    cohort, _ = load_cohort(config['evaluation']['cohort'], config['evaluation']['expected_cases'],
                            {k: ROOT / p for k, p in runtime.spec['catalogs'].items()})
    directory = output / f'seed-{seed}'
    level = config['duration']['compression_level']
    elapsed_before, began = state['seconds'], time.monotonic()
    elapsed = lambda: elapsed_before + time.monotonic() - began
    requested = [False]
    def request_stop(*_):
        requested[0] = True
    previous_handlers = {s: signal.signal(s, request_stop) for s in (signal.SIGTERM, signal.SIGINT)}
    def guard():
        if health is not None:
            health()
        require(elapsed() < config['limits']['run_seconds'], 'blueprint.duration: hard finalization deadline')
    job = state['jobs'].get(str(seed))
    if job is None:
        model, actor, critic = learners(seed, config['ppo'])
        sampler = torch.Generator().manual_seed(1_000_000 + seed)
        shuffle = torch.Generator().manual_seed(2_000_000 + seed)
        buffer = SuccessBuffer()
        streams = [Stream(runtime, i) for i in range(config['training']['streams'])]
        path = directory / 'checkpoints/0000.pt.gz'
        save_compact(path, snapshot(model, actor, critic, sampler, shuffle, seed, 0, streams, state['protocol_sha256'], buffer), level)
        job = dict(status='running', update=0, latest_checkpoint=reference(path, output), records=[], evaluations=[], removed_checkpoints=[])
        job['initialization'] = dict(update=0, model_sha256=digest(model.state_dict()),
            actor_optimizer_entries=len(actor.state), critic_optimizer_entries=len(critic.state),
            success_sequences=len(buffer.state['entries']), imported_checkpoint=None)
        state['jobs'][str(seed)] = job
    else:
        saved, buffer = restore_buffer(load(checked(job['latest_checkpoint'], output)))
        model, actor, critic, sampler, shuffle = restore_learners(saved, config['ppo'], state['protocol_sha256'])
        streams = [Stream.restore(runtime, s) for s in saved['streams']]
        require(saved['update'] == job['update'], 'blueprint.duration: durable update identity')
    state.update(status='running', stop_reason=None)
    state.pop('finished_at', None)
    state.setdefault('started_at', datetime.now(timezone.utc).isoformat())
    job['status'] = 'running'
    evidence, cache, partial = Evidence(output, job), SequenceCache(), {}
    write(output / 'state.json', state)

    def publish_evaluation():
        ordinal = job['update']
        if any(e['update'] == ordinal for e in job['evaluations']):
            return
        before_sampler, before_shuffle = digest(sampler.get_state()), digest(shuffle.get_state())
        rows = evaluate_cohort(runtime, model, seed, ordinal, directory / f'evaluation/{ordinal:07d}', guard, cohort)
        require(digest(sampler.get_state()) == before_sampler and digest(shuffle.get_state()) == before_shuffle,
                'blueprint.duration: evaluation altered training RNG')
        path = directory / f'evaluation/{ordinal:07d}/report.json'
        write(path, dict(schema='blueprint-ppo-evaluation-v1', seed=seed, update=ordinal, rows=rows,
            cases=[dict(generation_seed=row['generation_seed'],
                        **reference(path.parent / f'{row["start"]}-{row["sample"]}.pt.gz', output)) for row in rows]))
        job['evaluations'].append(dict(update=ordinal, seconds=elapsed(), **reference(path, output)))
        state['last_evaluation_seconds'] = elapsed()
        state['seconds'] = elapsed()
        write(output / 'state.json', state)
        print(dict(evaluation=ordinal, mean_area_reduction_percent=sum(r['policy_area_reduction_percent'] for r in rows) / len(rows)), flush=True)

    try:
        state['storage'] = storage_guard(output, config)
        if not job['evaluations']:
            publish_evaluation()
            if health is not None:
                health(initial=True)
            job['initialization']['evaluated_before_training'] = True
            write(output / 'state.json', state)
        reason = None
        while reason is None:
            reason = boundary_stop(elapsed(), config['duration']['seconds'], streams,
                                   job['update'], config['training']['updates'], requested[0])
            if reason:
                break
            if stop_after_updates is not None and job['update'] >= stop_after_updates:
                reason = 'readiness_checkpoint'
                break
            guard()
            check_sources(protocol)
            ordinal, partial = job['update'] + 1, {}
            payload = collect(model, streams, sampler, config, guard, partial)
            evidence.begin(ordinal, payload)
            before = buffer.snapshot()
            sequences = cache.resolve(buffer, evidence, ordinal)
            began_update = time.monotonic()
            result = update_success(model, actor, critic, shuffle, payload['samples'], payload['frontiers'],
                                    config['ppo'], config['success'], sequences, seed, ordinal)
            result['seconds'] = time.monotonic() - began_update
            buffer.mark_used(result['success']['selected_ids'])
            events = buffer.ingest(payload, ordinal, evidence, config['success'], config['ppo']['gamma'])
            payload.update(seed=seed, update=ordinal, protocol_sha256=state['protocol_sha256'], ppo=result,
                           buffer=dict(before=before, after=buffer.snapshot(), events=events))
            if buffer.state['entries'] and 'first_admission_update' not in job:
                job['first_admission_update'] = ordinal
            chunk = directory / f'chunks/{ordinal:07d}.pt.gz'
            save_compact(chunk, payload, level)
            checkpoint = directory / f'checkpoints/{ordinal:04d}.pt.gz'
            save_compact(checkpoint, snapshot(model, actor, critic, sampler, shuffle, seed, ordinal, streams,
                                             state['protocol_sha256'], buffer), level)
            record = dict(seed=seed, update=ordinal, chunk=reference(chunk, output), checkpoint=reference(checkpoint, output),
                actions=len(payload['samples']), area_reward=sum(s['reward'] for s in payload['samples']),
                elapsed_seconds=elapsed(), collection_seconds=payload['collection_seconds'],
                quality=training_quality(payload), ppo=result,
                buffer=dict(before_sha256=digest(before), after_sha256=digest(buffer.state),
                    entries=len(buffer.state['entries']), positive_commits=buffer.state['positive_commits'],
                    admissions=buffer.state['admissions'], duplicates=buffer.state['duplicates']))
            path = directory / f'updates/{ordinal:07d}.json'
            write(path, record)
            job['records'].append(reference(path, output))
            job.update(update=ordinal, latest_checkpoint=reference(checkpoint, output))
            state.update(seconds=elapsed(), actions=ordinal * config['training']['streams'] * config['training']['chunk_actions'],
                         positive_commits=buffer.state['positive_commits'], buffer_entries=len(buffer.state['entries']))
            write(output / 'state.json', state)
            print(dict(update=ordinal, seconds=round(state['seconds'], 2), actions=state['actions'],
                reward=record['area_reward'], entries=state['buffer_entries'], positive_labels=result['success']['positive_labels']), flush=True)
            if ordinal % config['evaluation']['interval_batches'] == 0:
                require(all(s.session is None for s in streams), 'blueprint.duration: evaluation at a complete episode boundary')
                publish_evaluation()
            prune_checkpoints(output, directory, job)
            if ordinal % 32 == 0:
                state['storage'] = storage_guard(output, config)
            state['seconds'] = elapsed()
            write(output / 'state.json', state)
        if reason != 'readiness_checkpoint':
            require(not buffer.state['active'] and all(s.session is None for s in streams), 'blueprint.duration: terminal boundary stop')
            publish_evaluation()
        state.update(status='complete' if reason in ('duration', 'update_limit') else 'paused', stop_reason=reason,
                     seconds=elapsed(), finished_at=datetime.now(timezone.utc).isoformat())
        job['status'] = state['status']
        state['storage'] = storage_guard(output, config)
        write(output / 'state.json', state)
        check_sources(protocol)
        print(dict(status=state['status'], stop_reason=reason, update=job['update'], seconds=state['seconds']), flush=True)
    except BaseException as error:
        save_compact(directory / 'interrupted.pt.gz', dict(schema='blueprint-duration-interrupted-v1', error=str(error), partial=partial,
            current=snapshot(model, actor, critic, sampler, shuffle, seed, job['update'], streams, state['protocol_sha256'], buffer)), level)
        state.update(status='failed', stop_reason=str(error), seconds=elapsed())
        job['status'] = 'failed'
        write(output / 'state.json', state)
        raise
    finally:
        for signum, handler in previous_handlers.items():
            signal.signal(signum, handler)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    prep = sub.add_parser('prepare')
    prep.add_argument('--config', type=Path, required=True)
    prep.add_argument('--native-library', type=Path, required=True)
    prep.add_argument('--output', type=Path, required=True)
    prep.add_argument('--readiness', action='store_true')
    prep.add_argument('--env-file', type=Path)
    prep.add_argument('--evaluation-cohort', type=Path, help='Explicit retained cohort manifest outside a frozen checkout')
    execute = sub.add_parser('run')
    execute.add_argument('--output', type=Path, required=True)
    execute.add_argument('--stop-after-updates', type=int)
    execute.add_argument('--env-file', type=Path)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    if args.command == 'prepare':
        prepare(args.config.resolve(), args.native_library.resolve(), args.output.resolve(), args.readiness, args.env_file,
                args.evaluation_cohort)
    else:
        run(args.output.resolve(), args.stop_after_updates, args.env_file)


if __name__ == '__main__':
    main()
