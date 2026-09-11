"""Verify every completed factorial run against its acknowledged W&B history."""
import argparse
import math
import os
from pathlib import Path
import time

from ppo_harness_comparison import tracking_jobs
from ppo_harness_environment import load_environment
from ppo_harness_monitor import snapshot
from ppo_harness_state import read, write, sha
from ppo_harness_tracking import metrics


def require(condition, message):
    if not condition:
        raise ValueError(f'ppo-combinations.tracking-audit: {message}')


def check(matrix_path, output):
    import wandb
    matrix = read(matrix_path)
    expected_cells = {'ppo-actor-critic-matrix-v1': 8, 'ppo-shared-capacity-matrix-v1': 4,
                      'ppo-learning-rate-matrix-v1': 2}.get(matrix['schema'])
    require(expected_cells is not None and matrix['status'] == 'complete'
            and not matrix['readiness'] and len(matrix['cells']) == expected_cells,
            'completed registered screen required')
    require(not output.exists(), f'fresh tracking audit output required: {output}')
    output.parent.mkdir(parents=True, exist_ok=True)
    report = dict(schema='ppo-combinations-tracking-audit-v1', status='running',
                  matrix_sha256=sha(matrix_path), source_sha256=sha(__file__), jobs=[])
    write(output, report)
    started = time.monotonic()
    try:
        api = wandb.Api(timeout=20)
        for cell in matrix['cells']:
            root = Path(cell['root'])
            require(sha(root/'protocol.json') == cell['protocol_sha256']
                    and sha(root/'state.json') == cell['state_sha256'], f'cell identity: {root}')
            protocol, tracking = read(root/'protocol.json'), read(root/'tracking.json')
            require(tracking['status'] == 'complete', f'local tracking completion: {root}')
            destination = protocol['config']['tracking']
            for job in tracking_jobs(snapshot(root), protocol):
                require(time.monotonic()-started < 900, 'tracking audit exceeded 900 seconds')
                saved = tracking['jobs'][job['id']]
                records = job['records']
                require(saved['complete'] and saved['finished_successfully']
                        and saved['remote_confirmed_step'] == saved['exported_records']-1 == len(records)-1,
                        f'local export ordinals: {root}/{job["id"]}')
                remote = api.run('/'.join((destination['entity'], destination['project'], saved['run_id'])))
                require(remote.state == 'finished' and remote.lastHistoryStep == len(records)-1,
                        f'remote completion: {saved["url"]}')
                require(remote.config['model'] == protocol['spec']['model'], f'remote model: {saved["url"]}')
                require(remote.config['ppo'] == protocol['config']['ppo'], f'remote PPO configuration: {saved["url"]}')
                rows = list(remote.scan_history(min_step=0, max_step=len(records), page_size=100))
                history = {row['_step']: row for row in rows}
                require(len(rows) == len(history) == len(records) and set(history) == set(range(len(records))),
                        f'remote ordinals: {saved["url"]}')
                count = 0
                for index, record in enumerate(records):
                    for key, value in metrics(record).items():
                        if type(value) not in (int, float):
                            continue
                        require(key in history[index] and math.isclose(history[index][key], value, rel_tol=1e-8, abs_tol=1e-9),
                                f'remote scalar: {saved["url"]}, ordinal={index}, metric={key}')
                        count += 1
                detail = dict(cell=cell['name'], seed=job['seed'], run_id=saved['run_id'], url=saved['url'],
                              remote_state=remote.state, records=len(records), scalars_verified=count,
                              tracking_sha256=sha(root/'tracking.json'),
                              prior_local_diagnostic=tracking.get('diagnostic'))
                report['jobs'].append(detail)
                write(output, report)
                print(dict(cell=cell['name'], seed=job['seed'], scalars_verified=count), flush=True)
        require(len(report['jobs']) == 2 * expected_cells, 'two completed remote runs per cell required')
        report.update(status='complete', records=sum(j['records'] for j in report['jobs']),
                      scalars_verified=sum(j['scalars_verified'] for j in report['jobs']))
    except BaseException as error:
        message = str(error)
        secret = os.environ.get('WANDB_API_KEY')
        if secret:
            message = message.replace(secret, '[redacted]')
        report.update(status='failed', diagnostic=dict(code='ppo-combinations.tracking-audit-failed',
                      severity='error', stage='tracking-audit', entity=str(matrix_path), message=message))
        raise ValueError(message) from None
    finally:
        report['seconds'] = time.monotonic()-started
        write(output, report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--env-file', type=Path, required=True)
    args = parser.parse_args()
    load_environment(args.env_file.resolve())
    check(args.matrix.resolve(), args.output.resolve())
