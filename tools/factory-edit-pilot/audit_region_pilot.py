"""Audit a completed bounded region pilot and its acknowledged online scalars."""
import argparse
import os
from pathlib import Path
import time

os.environ.setdefault('CUBLAS_WORKSPACE_CONFIG', ':4096:8')


def audit(root, output, env_file):
    import torch
    import wandb
    from native_factory_runtime import FactoryRuntime
    from ppo_area_audit import verify_result
    from ppo_harness_environment import load_environment
    from ppo_harness_run import learners
    from ppo_harness_state import read, write, sha, require, checked_path, check_sources, restore_checkpoint
    from ppo_harness_tracking import metrics
    from ppo_harness_comparison import tracking_jobs
    from ppo_harness_monitor import snapshot
    import math

    load_environment(env_file)
    torch.set_num_threads(1)
    protocol, state = read(root / 'protocol.json'), read(root / 'state.json')
    require(state['status'] == 'complete', 'audit-terminal', 'completed schedule required')
    check_sources(protocol)
    output.mkdir(parents=True, exist_ok=False)
    began = time.monotonic()
    runtime = FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])
    checks, identities = 0, set()
    try:
        for job in state['jobs']:
            for reference in job['records']:
                record = read(checked_path(root, reference))
                for key in ('checkpoint', 'behavior_checkpoint'):
                    identities.add(str(checked_path(root, record[key])))
                if record['phase']['kind'] == 'evaluation':
                    for result in record['results']:
                        verify_result(runtime, result, output)
                        checks += 1
            model_set = learners(protocol, job['seed'])
            restore_checkpoint(root, job['checkpoint'], model_set, state['protocol_sha256'], job['id'], job['completed_batches'])
    finally:
        runtime.close()
    (output / 'best.json').unlink(missing_ok=True)
    destination = protocol['config']['tracking']
    cloud = []
    for job in tracking_jobs(snapshot(root), protocol):
        exported = read(root / 'tracking.json')['jobs'][job['id']]
        remote = wandb.Api(timeout=15).run(f"{destination['entity']}/{destination['project']}/{exported['run_id']}")
        require(remote.state == 'finished', 'audit-wandb-state', f'expected finished, got {remote.state}')
        history = {int(r['_step']): r for r in remote.scan_history(page_size=100)}
        require(set(history) == set(range(len(job['records']))), 'audit-wandb-records', 'missing or duplicate cloud ordinals')
        scalars = 0
        for index, record in enumerate(job['records']):
            for key, value in metrics(record).items():
                require(key in history[index] and math.isclose(history[index][key], value, rel_tol=1e-8, abs_tol=1e-8),
                        'audit-wandb-metric', f'cloud scalar differs: {index}/{key}')
                scalars += 1
        cloud.append(dict(url=remote.url, state=remote.state, records=len(history), scalars=scalars,
                          summary={k: remote.summary.get(k) for k in ('source_status', 'training_complete', 'termination_reason')}))
    result = dict(schema='ppo-region-final-audit-v1', status='complete', protocol_sha256=sha(root / 'protocol.json'),
                  source_hashes_verified=True, checkpoints_verified=len(identities), native_evaluation_results=checks,
                  evaluation_rewards_verified=True, final_checkpoint_restored=True, wandb=cloud,
                  seconds=time.monotonic() - began)
    write(output / 'report.json', result)
    print(result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--env-file', type=Path, required=True)
    args = parser.parse_args()
    audit(args.run.resolve(), args.output.resolve(), args.env_file.resolve())
