"""W&B export of committed records with stable job IDs and explicit custom axes."""
import hashlib
import os
from pathlib import Path

from ppo_harness_state import require, read, write


def run_id(protocol_hash, job):
    return hashlib.sha256(f'{protocol_hash}:{job}'.encode()).hexdigest()[:24]


def metrics(record):
    value = dict(**{'train/actions': record['selected_training_actions'],
                    'train/batches': record['phase']['batch'], 'time/elapsed_seconds': record['elapsed_seconds']})
    if 'parent_batches' in record:
        value.update({'train/cumulative_batches': record['parent_batches'] + record['phase']['batch'],
                      'train/cumulative_actions': record['parent_training_actions'] + record['selected_training_actions']})
    quality = record['quality']
    if record['phase']['kind'] in ('evaluation', 'comparison'):
        outcomes = dict(area_reduction_percent=quality['mean_area_reduction_percent'],
                        delta_from_initial_pp=record['paired_to_initial']['paired_delta_pp'])
        for axis in ('actions', 'batches', 'seconds'):
            prefix = 'outcome' if record['phase']['kind'] == 'comparison' else 'evaluation/all_admitted'
            value.update({f'{prefix}/by_{axis}/{name}': number for name, number in outcomes.items() if number is not None})
        prefix = 'comparison' if record['phase']['kind'] == 'comparison' else 'evaluation'
        value.update({f'{prefix}/accepted': quality['accepted'], f'{prefix}/attempted': quality['attempted'],
                      f'{prefix}/coverage_fraction': quality['accepted'] / quality['attempted'],
                      f'{prefix}/paired_cases': len(record['paired_to_initial']['shared_seeds'])})
        if record['phase']['kind'] == 'comparison':
            return value
    else:
        for name in ('mean_area_reduction_percent', 'accepted', 'attempted', 'generation_rejected',
                     'unique_requests', 'unique_geometry', 'distinct_rates'):
            if quality[name] is not None:
                value[f'training/{name}'] = quality[name]
        ppo = record['ppo']
        value.update({f'ppo/{key}': number for key, number in ppo['epochs'][-1].items()
                      if type(number) in (int, float)})
        value.update({'ppo/optimizer_steps': ppo['optimizer_steps'], 'ppo/epochs': len(ppo['epochs']),
                      'ppo/kl_stopped': int(ppo['kl_stopped']), 'ppo/raw_advantage_std': ppo['raw_advantage_std']})
        value.update({f'timing/{key}': number for key, number in ppo['timing'].items()})
        value.update({'timing/update_seconds': record['update_seconds'],
                      'timing/checkpoint_seconds': record['checkpoint_seconds']})
    prefix = 'training' if record['phase']['kind'] == 'training' else 'evaluation'
    for name in ('routing_detailed_actions', 'failed_actions', 'routing_budget_failed_actions',
                 'routing_budget_failure_fraction', 'total_route_expansions', 'total_executor_seconds',
                 'total_preflight_expansions', 'first_attempt_budget_failures',
                 'mean_reward', 'mean_area_reward', 'mean_failure_cost'):
        if quality.get(name) is not None:
            value[f'{prefix}/{name}'] = quality[name]
    if quality.get('selected') and 'failed_actions' in quality:
        value[f'{prefix}/failure_fraction'] = quality['failed_actions'] / quality['selected']
    value.update({f'{prefix}/failure_code/{code}': count for code, count in quality.get('routing_failure_codes', {}).items()})
    value.update({'timing/phase_seconds': record['phase_seconds'], 'timing/collection_seconds': record['collection']['seconds'],
                  'throughput/selections_per_second': record['selected_per_second'],
                  'timing/summed_generation_seconds': quality['total_generation_seconds'],
                  'timing/summed_episode_seconds': quality['total_episode_seconds']})
    return value


class Tracker:
    def __init__(self, root, protocol, protocol_hash, session):
        self.root, self.protocol, self.protocol_hash, self.session = root, protocol, protocol_hash, session
        self.path, self.active, self.active_job, self.next_step = root / 'tracking.json', None, None, 0
        self.value = read(self.path) if self.path.exists() else dict(schema='ppo-wandb-export-v1', jobs={})
        self.value.update(status='running', session=session, protocol_sha256=protocol_hash)
        self.config = protocol['config']['tracking']
        self.enabled = self.config['backend'] == 'wandb'

    def publish(self):
        import time
        self.value['updated_epoch'] = time.time()
        write(self.path, self.value)

    def start(self, job):
        import wandb
        directory = self.root / 'tracking' / self.session / job['id']
        directory.mkdir(parents=True, exist_ok=True)
        online = self.config['mode'] == 'online'
        identifier = run_id(self.protocol_hash, job['id'])
        model = self.protocol['spec'].get('model')
        model_label = '' if model is None else f"cnn-{model['cnn_layers']}-d{','.join(map(str, model['cnn_dilations']))}--"
        self.active = wandb.init(entity=self.config['entity'] or None, project=self.config['project'],
            id=identifier, name=model_label + job['id'], group=f"ppo-{self.protocol['config']['purpose']}-{self.protocol_hash[:12]}",
            job_type=self.protocol['config']['purpose'],
            mode=self.config['mode'], resume='allow' if online else None, dir=str(directory),
            save_code=False, settings=wandb.Settings(console='off', disable_code=True, x_disable_meta=True,
                x_disable_stats=True, x_disable_machine_info=True, init_timeout=20, finish_timeout=10),
            config=dict(protocol_sha256=self.protocol_hash, candidate=job['candidate'], seed=job['seed'],
                        policy=self.protocol['spec']['policy'], model=model, continuation=self.protocol.get('continuation'),
                        reward=self.protocol['config'].get('reward'),
                        selection=self.protocol['spec'].get('selection'),
                        ppo=self.protocol['config']['ppo'],
                        training=self.protocol['config']['training'], evaluation=self.protocol['config']['evaluation'],
                        execution=self.protocol['config']['execution']))
        try:
            if online:
                # A resumed SDK service can initially report step zero. Use
                # acknowledged remote history as a lower bound for its cursor.
                remote = wandb.Api(timeout=10).run(f"{self.config['entity']}/{self.config['project']}/{identifier}")
                self.next_step = max(self.active.step, remote.lastHistoryStep + 1)
            else:
                self.next_step = 0
        except Exception:
            try:
                self.active.finish(exit_code=1)
            finally:
                self.active = None
            raise
        self.active_job = job['id']
        for key in ('train/actions', 'train/batches', 'time/elapsed_seconds'):
            self.active.define_metric(key)
        for prefix in ('training', 'ppo', 'timing', 'throughput', 'evaluation', 'comparison'):
            self.active.define_metric(f'{prefix}/*', step_metric='train/actions')
        for axis, step in [('actions', 'train/actions'), ('batches', 'train/batches'), ('seconds', 'time/elapsed_seconds')]:
            self.active.define_metric(f'outcome/by_{axis}/*', step_metric=step)
            self.active.define_metric(f'evaluation/all_admitted/by_{axis}/*', step_metric=step)
        self.value['jobs'][job['id']] = dict(run_id=identifier, url=self.active.url if online else None,
            directory=str(directory), mode=self.config['mode'], exported_records=self.next_step,
            complete=False, remote_confirmed_step=None, source_status=job['status'], export_session=self.session)
        self.publish()

    def finish(self, complete, *, failed=False, reason=None):
        if self.active is None:
            return
        import wandb
        job = self.active_job
        record = self.value['jobs'][job]
        self.active.summary['training_complete'] = complete
        self.active.summary['source_status'] = record['source_status']
        # Schedule completion and successful process termination are independent.
        # A bounded training session may end normally before the batch ceiling.
        reason = reason or ('schedule-complete' if complete else 'session-ended')
        self.active.summary['termination_reason'] = reason
        self.active.finish(exit_code=1 if failed else 0)
        self.active = None
        self.active_job = None
        if not failed and self.config['mode'] == 'online':
            remote = wandb.Api(timeout=10).run(f"{self.config['entity']}/{self.config['project']}/{record['run_id']}")
            record['remote_confirmed_step'] = remote.lastHistoryStep
            require(record['remote_confirmed_step'] >= record['exported_records'] - 1, 'tracking-unconfirmed',
                    'W&B has not confirmed the final committed record', 'tracking', job)
        record['complete'] = complete
        record.update(finished_successfully=not failed, termination_reason=reason)
        self.publish()

    def update(self, jobs):
        if not self.enabled:
            self.value.update(status='disabled')
            self.publish()
            return
        for job in jobs:
            previous = self.value['jobs'].get(job['id'], {})
            if self.config['mode'] == 'online' and previous.get('complete'):
                confirmed = previous.get('remote_confirmed_step', -1)
                require(confirmed < len(job['records']), 'tracking-ahead',
                        'remote history is ahead of durable training state', 'tracking', job['id'])
                if confirmed == len(job['records']) - 1:
                    continue
            if not job['records']:
                continue
            # Online completion is recorded only after remote history confirms
            # the final ordinal. Offline restarts rebuild one canonical export.
            if self.config['mode'] == 'offline' and previous.get('complete') and previous.get('export_session') == self.session and previous.get('exported_records', 0) >= len(job['records']):
                continue
            if self.active is None or self.active_job != job['id']:
                self.finish(complete=False)
                self.start(job)
            require(self.next_step <= len(job['records']), 'tracking-ahead', 'remote history is ahead of durable training state', 'tracking', job['id'])
            for index in range(self.next_step, len(job['records'])):
                self.active.log(metrics(job['records'][index]), step=index, commit=True)
                self.next_step = index + 1
            record = self.value['jobs'][job['id']]
            record.update(exported_records=self.next_step, source_status=job['status'])
            self.publish()
            if job['status'] == 'complete':
                self.finish(complete=True)

    def failed(self, error):
        text = str(error)
        secret = os.environ.get('WANDB_API_KEY')
        if secret:
            text = text.replace(secret, '[redacted]')
        self.value.update(status='failed', diagnostic=dict(code='ppo-harness.tracking-failed', severity='error',
                          stage='tracking', entity=self.active_job, message=text))
        self.publish()
