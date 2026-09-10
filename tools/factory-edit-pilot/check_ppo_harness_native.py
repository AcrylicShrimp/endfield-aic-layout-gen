"""Bounded readiness audit: native validity, one continuation per job and W&B scalars."""
import argparse
import os
from pathlib import Path
import math
import time

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')

import torch

from native_factory_runtime import FactoryRuntime
from ppo_area_audit import verify_result
from ppo_area_update import load_experience
from ppo_harness_environment import load_environment
from ppo_harness_run import learners
from ppo_harness_state import read, write, sha, require, load_state, checked_path, restore_checkpoint, check_sources
from ppo_harness_tracking import metrics
from ppo_harness_comparison import tracking_jobs
from ppo_harness_monitor import snapshot
from ppo_training_batch import prepare, distribution, critic_values
from ppo_training_update import update


def check(root, output, cloud):
    require(not output.exists(), 'check-output', f'fresh output required: {output}')
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    if torch.cuda.is_available():
        torch.backends.cuda.matmul.fp32_precision = 'ieee'
        torch.backends.cudnn.conv.fp32_precision = 'ieee'
        torch.backends.cudnn.benchmark = False
    protocol, state = read(root / 'protocol.json'), load_state(root)
    check_sources(protocol)
    require(state['status'] == 'complete' and protocol['config']['purpose'] == 'readiness', 'check-scope',
            'this bounded audit requires a completed readiness run')
    output.mkdir()
    report = dict(schema='ppo-harness-native-readiness-v1', status='running', source_sha256=sha(__file__),
                  protocol_sha256=state['protocol_sha256'], jobs=[], max_seconds=180)
    began = time.monotonic()
    def guard():
        require(time.monotonic() - began < 180, 'check-timeout', 'native readiness audit exceeded 180 seconds')
    cloud_jobs = {j['id']: j for j in tracking_jobs(snapshot(root), protocol)} if cloud else {}
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    try:
        for job in state['jobs']:
            guard()
            records = [read(checked_path(root, r)) for r in job['records']]
            total = transient = 0
            for record in records:
                for result in record['results']:
                    guard()
                    task = read(Path(result['result_path']).with_name('task.json'))
                    if task['training'] and not task['retain_details']:
                        require('experience' not in result and
                                not Path(result['result_path']).with_name('experience.pt.gz').exists(),
                                'unexpected-experience-archive', 'transient training must not write experience archives',
                                'audit', result['result_path'])
                        require(result['status'] != 'complete' or not
                                (Path(result['row']['directory']) / 'episode.json.gz').exists(),
                                'unexpected-episode-trace', 'transient training must not retain a full episode trace',
                                'audit', result['result_path'])
                        transient += 1
                        continue
                    verify_result(runtime, result, output)
                    total += 1
            # Reconstruct one update, including twice-optimized starts when enabled;
            # this standalone check is never part of the normal learner.
            continuation_batch = 3 if 'start_curriculum' in protocol['config'] else 2
            record = next(r for r in records if r['phase'] == dict(kind='training', batch=continuation_batch))
            model_set = learners(protocol, job['seed'])
            saved = restore_checkpoint(root, record['behavior_checkpoint'], model_set, state['protocol_sha256'], job['id'], continuation_batch - 1)
            require(saved['actor_optimizer']['state'] and saved['critic_optimizer']['state'], 'empty-adam', 'nonempty optimizer checkpoint required')
            sample = next(r for r in record['results'] if r['status'] == 'complete' and
                          (continuation_batch == 2 or r['generation']['start_curriculum']['depth'] == 2))
            experience = load_experience(sample)
            config = dict(protocol['spec']['selection'], **protocol['config']['ppo'], reward=protocol['config']['reward'])
            if hasattr(model_set[1], 'treatment'):
                config['actor_critic'] = model_set[1].treatment
            from ppo_training_transfer import move_prepared
            batch = move_prepared(prepare(experience['steps'], config, model_set[0].width, 'cpu'), next(model_set[0].parameters()).device)
            with torch.no_grad():
                torch.testing.assert_close(distribution(model_set[0], batch, config)[0], batch['old_logp'], rtol=5e-5, atol=5e-6)
                torch.testing.assert_close(critic_values(model_set[1], batch), batch['old_values'], rtol=5e-5, atol=5e-6)
            experiences = [load_experience(r) if r['status'] == 'complete' else None for r in record['results']]
            actual = update(*model_set, record['results'], config, protocol['config']['execution']['microbatch'], guard,
                            experiences=experiences)
            for key in ('policy_after_sha256', 'critic_after_sha256', 'actor_optimizer_sha256',
                        'critic_optimizer_sha256', 'shuffle_rng_sha256', 'epochs'):
                require(actual[key] == record['ppo'][key], 'continuation-mismatch', f'exact saved continuation differs: {key}', entity=job['id'])
            detail = dict(job=job['id'], native_results=total, transient_results_without_archives=transient,
                          behavior_probabilities_and_values_match=True,
                          one_nonempty_checkpoint_continuation_exact=True, reconstructed_batch=continuation_batch)
            if 'actor_critic' in actual:
                require(actual['critic_tail']['epochs'] and actual['critic_tail']['actor_parameters_and_optimizer_unchanged'],
                        'critic-tail-missing', 'readiness must exercise a critic-only tail', entity=job['id'])
                for key in ('actor_epochs', 'critic_epochs', 'actor_optimizer_steps', 'critic_optimizer_steps'):
                    require(actual[key] == record['ppo'][key], 'critic-tail-count', f'tail differs: {key}', entity=job['id'])
                detail.update(critic_tail_preserves_policy=True, critic_history_matches_actor=True,
                              actor_epochs=actual['actor_epochs'], critic_epochs=actual['critic_epochs'])
            if 'start_curriculum' in protocol['config']:
                from audit_ppo_start_curriculum import check_records
                detail['curriculum'] = check_records(root, protocol, records)
            if cloud:
                import wandb
                tracking = read(root / 'tracking.json')['jobs'][job['id']]
                destination = protocol['config']['tracking']
                run = wandb.Api(timeout=15).run(f"{destination['entity']}/{destination['project']}/{tracking['run_id']}")
                remote = {int(r['_step']): r for r in run.scan_history(page_size=100)}
                cloud_records = cloud_jobs[job['id']]['records']
                require(set(remote) == set(range(len(cloud_records))), 'cloud-records', 'W&B has missing or duplicate durable ordinals', entity=job['id'])
                count = 0
                for index, record in enumerate(cloud_records):
                    for key, value in metrics(record).items():
                        require(key in remote[index] and math.isclose(remote[index][key], value, rel_tol=1e-8, abs_tol=1e-8),
                                'cloud-metric', f'W&B differs from committed scalar: {key}', entity=job['id'])
                        count += 1
                detail.update(wandb_scalars_verified=count, wandb_url=tracking['url'])
            report['jobs'].append(detail)
            write(output / 'report.json', report)
            print(detail, flush=True)
    finally:
        runtime.close()
    (output / 'best.json').unlink(missing_ok=True)
    report.update(status='complete', seconds=time.monotonic() - began)
    write(output / 'report.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--check-wandb', action='store_true')
    args = parser.parse_args()
    load_environment(Path(__file__).resolve().parents[2] / '.env')
    check(args.run.resolve(), args.output.resolve(), args.check_wandb)
