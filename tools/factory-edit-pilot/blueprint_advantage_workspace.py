"""Create or verify a W&B view scoped to one registered fresh-input advantage matrix."""
import argparse
from importlib.metadata import version
from pathlib import Path

from blueprint_duration_workspace import PANELS
from blueprint_duration_metrics import evaluation_metrics
from ppo_harness_environment import load_environment, tracking_destination
from ppo_harness_state import read, write


def evaluation_rows(history, metric, delta):
    # The history API can include training rows with null evaluation fields.
    rows = [row for row in history if row.get(metric) is not None]
    if any(row.get(k) is None for row in rows for k in ('train/batches', 'train/actions', delta)):
        raise ValueError('online evaluation is missing its axes or paired baseline delta')
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--verify-url')
    parser.add_argument('--env-file', type=Path, default=Path(__file__).resolve().parents[2] / '.env')
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f'output already exists: {args.output}')
    load_environment(args.env_file)
    destination = tracking_destination(required=True)
    import wandb
    import wandb_workspaces.workspaces as ws
    import wandb_workspaces.reports.v2 as wr
    matrix = read(args.matrix / 'matrix.json')
    complete = read(args.matrix / 'state.json')['status'] == 'complete'
    matrix_hash = matrix['matrix_sha256']
    api = wandb.Api(timeout=15)
    runs = list(api.runs(f'{destination["entity"]}/{destination["project"]}',
                        filters={'config.experiment.matrix_sha256': matrix_hash}))
    if not runs:
        parser.error('matrix has no online runs yet')
    expected_arms = {(a['variant'], a['seed']) for a in matrix['arms']}
    actual_arms = {(r.config['experiment']['variant'], r.config['seed']) for r in runs}
    if not actual_arms <= expected_arms or len(actual_arms) != len(runs):
        parser.error('remote matrix run identities differ from the registered arms')
    if complete and (actual_arms != expected_arms or any(r.state != 'finished' for r in runs)):
        parser.error('completed matrix requires all six online runs to be finished')
    metric = 'evaluation/all_admitted/by_batches/area_reduction_percent'
    delta = 'evaluation/all_admitted/by_batches/delta_from_initial_pp'
    histories = {}
    for run in runs:
        arm = next(a for a in matrix['arms'] if
                   (a['variant'], a['seed']) == (run.config['experiment']['variant'], run.config['seed']))
        if run.config['protocol_sha256'] != arm['protocol']['sha256']:
            parser.error(f'online run protocol differs: {run.id}')
        rows = evaluation_rows(run.scan_history(keys=['train/batches', 'train/actions', metric, delta],
                                               page_size=200), metric, delta)
        if not rows or rows[0]['train/batches'] != 0:
            parser.error(f'missing actual untrained evaluation: {run.id}')
        histories[run.id] = dict(variant=run.config['experiment']['variant'], seed=run.config['seed'],
            state=run.state, evaluation_points=len(rows), first_batch=0, last_batch=rows[-1]['train/batches'])
        if complete:
            directory = args.matrix / arm['directory']
            job = read(directory / 'state.json')['jobs'][str(arm['seed'])]
            reports = [read(directory / ref['path']) for ref in job['evaluations']]
            if [r['train/batches'] for r in rows] != [r['update'] for r in reports]:
                parser.error(f'online evaluation cadence differs: {run.id}')
            config = read(directory / 'protocol.json')['config']
            actions_per_batch = config['training']['streams'] * config['training']['chunk_actions']
            for observed, report in zip(rows, reports):
                expected = evaluation_metrics(report['rows'], reports[0]['rows'])
                if (any(abs(observed[k] - expected[k]) > 1e-6 for k in (metric, delta)) or
                        observed['train/actions'] != report['update'] * actions_per_batch):
                    parser.error(f'online evaluation values differ: {run.id}, batch {report["update"]}')
            histories[run.id]['all_durable_evaluation_values_verified'] = True
    panels = PANELS + [(
        'Fresh training inputs and credit', key, 'train/batches', [key]) for key in (
            'training/generated_episodes', 'training/generation_rejections', 'training/generated_products',
            'training/input_access_seconds', 'ppo/raw_advantage_std', 'ppo/actor_advantage_std',
            'ppo/advantage_scale', 'ppo/zero_reward_batch', 'ppo/value_mae')] + [('Compute cost', 'timing/collection_seconds', 'train/batches', ['timing/collection_seconds']),
                       ('Compute cost', 'timing/update_seconds', 'train/batches', ['timing/update_seconds'])]
    sections = [ws.Section(name=group, is_open=True, panels=[
        wr.LinePlot(title=title, x=axis, y=keys,
                    title_x={'time/elapsed_seconds': 'Training elapsed seconds',
                             'train/actions': 'Training actions', 'train/batches': 'Training batches'}[axis])
        for section, title, axis, keys in panels if section == group])
        for group in dict.fromkeys(group for group, _, _, _ in panels)]
    expression = f'Config("experiment.matrix_sha256") == "{matrix_hash}"'
    if args.verify_url:
        workspace = ws.Workspace.from_url(args.verify_url)
    else:
        workspace = ws.Workspace(**destination, name='Fresh random factories: advantage standardization vs centering',
            sections=sections, auto_generate_panels=False,
            runset_settings=ws.RunsetSettings(filters=expression))
        workspace.save()
    restored = ws.Workspace.from_url(workspace.url)
    name = lambda value: value if isinstance(value, str) else value.name
    actual = [(p.title, name(p.x), [name(k) for k in p.y]) for s in restored.sections for p in s.panels]
    if actual != [(title, axis, keys) for _, title, axis, keys in panels]:
        raise RuntimeError(f'advantage workspace panel readback differs: {workspace.url}')
    if (restored.entity, restored.project) != (destination['entity'], destination['project']):
        raise RuntimeError('advantage workspace destination differs')
    if matrix_hash not in str(restored.runset_settings.filters):
        raise RuntimeError('advantage workspace matrix filter differs')
    write(args.output, dict(schema='blueprint-advantage-workspace-v1', url=workspace.url,
        matrix_sha256=matrix_hash, panels=panels, panel_readback_verified=True,
        matrix_filter_verified=True, all_arms_online=actual_arms == expected_arms,
        histories=histories, wandb_workspaces_version=version('wandb-workspaces')))
    print(workspace.url)


if __name__ == '__main__':
    main()
