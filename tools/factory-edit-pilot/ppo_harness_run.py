"""Transactional same-policy PPO coordinator, supervised by the public CLI."""
import argparse
import os
from pathlib import Path
import signal
import time

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')

import torch

from ppo_area_model import models
from ppo_harness_pool import Pool
from ppo_harness_schedule import phase_tasks, quality, evaluation_pairs, cleanup_phase
from ppo_harness_state import (SCHEMA, require, read, write, sha, diagnostic, load_state, check_sources,
                               save_checkpoint, restore_checkpoint, manifest, checked_path, commit_phase, mkdir)
from ppo_training_update import update


class Progress:
    def __init__(self, root, session):
        self.path, self.last_write = root / 'progress.json', 0.
        self.value = dict(schema=SCHEMA, session=session, stage='starting', last_progress_epoch=time.time())
        self.job_started, self.job_prior = time.monotonic(), 0.

    def job(self, value):
        self.job_started, self.job_prior = time.monotonic(), value['elapsed_seconds']
        self.value.update(job=value['id'], candidate=value['candidate'], seed=value['seed'])

    def elapsed(self):
        return self.job_prior + time.monotonic() - self.job_started

    def __call__(self, advanced=True, **values):
        now = time.time()
        if advanced:
            self.value['last_progress_epoch'] = now
        self.value.update(values, updated_epoch=now, job_elapsed_seconds=self.elapsed())
        if advanced or time.monotonic() - self.last_write >= 1:
            write(self.path, self.value)
            self.last_write = time.monotonic()


def learners(protocol, seed):
    device = protocol['config']['execution']['device']
    require(device != 'cuda' or torch.cuda.is_available(), 'cuda-unavailable', 'configured CUDA learner is unavailable', 'learner')
    actor, critic = models(protocol['spec'], seed)
    actor, critic = actor.to(device), critic.to(device)
    learning_rate = protocol['config']['ppo']['learning_rate']
    aopt, copt = (torch.optim.Adam(model.parameters(), lr=learning_rate) for model in (actor, critic))
    return actor, critic, aopt, copt, torch.Generator().manual_seed(seed + 31000000)


def run(root, session):
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    if torch.cuda.is_available():
        torch.backends.cuda.matmul.fp32_precision = 'ieee'
        torch.backends.cudnn.conv.fp32_precision = 'ieee'
        torch.backends.cudnn.benchmark = False
    def interrupted(signum, _):
        raise InterruptedError(f'coordinator signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    protocol = read(root / 'protocol.json')
    check_sources(protocol)
    state, progress = load_state(root), Progress(root, session)
    protocol_hash, config = state['protocol_sha256'], protocol['config']
    progress(stage='initialization')
    try:
        while state['cursor'] < len(protocol['jobs']):
            index = state['cursor']
            scheduled = protocol['jobs'][index]
            model_set = learners(protocol, scheduled['seed'])
            directory = root / scheduled['id']
            mkdir(directory)
            if len(state['jobs']) == index:
                job = dict(scheduled, status='running', next_phase=0, records=[], selected_training_actions=0,
                           elapsed_seconds=0., completed_batches=0)
                version = save_checkpoint(root, directory / f'initial-{session}.pt', model_set,
                                          protocol_hash, job['id'], 0)
                job['checkpoint'] = version
                state['jobs'].append(job)
                write(root / 'state.json', state)
            else:
                job = state['jobs'][index]
                require(all(job[k] == scheduled[k] for k in ('id', 'candidate', 'seed')), 'job-identity', 'schedule/job mismatch', 'resume')
                restore_checkpoint(root, job['checkpoint'], model_set, protocol_hash, job['id'], job['completed_batches'])
                if job['records']:
                    previous_path = checked_path(root, job['records'][-1])
                    previous = read(previous_path)
                    cleanup_phase(root, previous_path.parent, previous['results'],
                                  retain_training=previous['phase']['batch'] in config['retention']['training_batches'])
            progress.job(job)
            progress(stage='worker-startup', committed_batches=job['completed_batches'])
            with Pool(root, directory / f'pool-{session}', config, progress) as pool:
                while job['next_phase'] < len(protocol['phases']):
                    phase = protocol['phases'][job['next_phase']]
                    progress(stage='phase-setup', phase=phase['kind'], batch=phase['batch'], completed=0, total=0)
                    began = time.monotonic()
                    location, tasks, cached, attempts = phase_tasks(root, protocol, protocol_hash, job, phase)
                    fresh, collection = pool.collect([path for _, path in tasks])
                    results_by_index = dict(cached)
                    results_by_index.update({index: result for (index, _), result in zip(tasks, fresh)})
                    results = [results_by_index[i] for i in sorted(results_by_index)]
                    summary = quality(results)
                    record = dict(schema=SCHEMA, job=job['id'], candidate=job['candidate'], seed=job['seed'],
                                  phase=phase, results=results, quality=summary, collection=collection,
                                  reused_results=len(cached), attempts=attempts, behavior_checkpoint=job['checkpoint'])
                    if phase['kind'] == 'training':
                        require(summary['accepted'] >= 2, 'insufficient-episodes', 'fewer than two complete training episodes', 'learner', location)
                        progress(stage='learner', completed=summary['accepted'], total=summary['attempted'])
                        began_update = time.monotonic()
                        selection = dict(protocol['spec']['selection'], **config['ppo'])
                        metrics = update(*model_set, results, selection, config['execution']['microbatch'],
                                         guard=lambda: progress(stage='learner'))
                        record['update_seconds'] = time.monotonic() - began_update
                        metrics.pop('credit')
                        record['ppo'] = metrics
                        job['selected_training_actions'] += metrics['selected']
                        job['completed_batches'] = phase['batch']
                        progress(stage='checkpoint')
                        began_checkpoint = time.monotonic()
                        version = save_checkpoint(root, location / f'checkpoint-{session}.pt', model_set,
                                                  protocol_hash, job['id'], phase['batch'])
                        record['checkpoint_seconds'] = time.monotonic() - began_checkpoint
                    else:
                        require(summary['accepted'] > 0, 'empty-evaluation', 'no complete held-out evaluations', 'evaluation', location)
                        version = job['checkpoint']
                        baseline_record = next((r for r in job['records'] if r['phase'] == dict(kind='evaluation', batch=0)), None)
                        baseline = read(checked_path(root, baseline_record))['results'] if baseline_record else results
                        record['paired_to_initial'] = evaluation_pairs(results, baseline)
                    job['elapsed_seconds'] = progress.elapsed()
                    record.update(checkpoint=version, selected_training_actions=job['selected_training_actions'],
                                  elapsed_seconds=job['elapsed_seconds'], phase_seconds=time.monotonic() - began,
                                  measured_epoch=time.time())
                    record['selected_per_second'] = summary['selected'] / max(record['phase_seconds'], 1e-9)
                    path = location / f'record-{session}.json'
                    write(path, record)
                    reference = dict(**manifest(root, path), phase=phase)
                    commit_phase(root, state, index, version, reference)
                    progress(stage='committed', committed_batches=job['completed_batches'],
                             committed_actions=job['selected_training_actions'])
                    cleanup_phase(root, location, results,
                                  retain_training=phase['batch'] in config['retention']['training_batches'])
                    print(dict(job=job['id'], phase=phase, accepted=summary['accepted'],
                               committed_actions=job['selected_training_actions']), flush=True)
            job.update(status='complete', elapsed_seconds=progress.elapsed())
            state['cursor'] += 1
            write(root / 'state.json', state)
        state['status'] = 'complete'
        write(root / 'state.json', state)
        progress(stage='complete', status='complete')
    except BaseException as error:
        request = root / 'stop-request.json'
        stop = read(request) if request.is_file() else {}
        if isinstance(error, InterruptedError) and stop.get('session') == session and stop.get('reason') in ('duration-reached', 'user-stop'):
            progress(stage='stopped', status='stopped', diagnostic=dict(code='ppo-harness.' + stop['reason'],
                     severity='info', stage='coordinator', entity=str(root), message='Training session ended normally; last committed checkpoint retained'))
            return
        progress(stage='failed', status='failed', diagnostic=diagnostic(error, 'coordinator', root))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--session', required=True)
    args = parser.parse_args()
    run(args.run.resolve(), args.session)
