"""Create a W&B view with panels for the duration observer's actual metric keys."""
import argparse
from importlib.metadata import version
from pathlib import Path

from ppo_harness_environment import load_environment, tracking_destination
from ppo_harness_state import read, sha, write

PANELS = [
    ('Training', 'Area reward per update', ['training/area_reward']),
    ('Training', 'Positive area commits', ['buffer/positive_commits']),
    ('Training', 'Retained success sequences', ['buffer/entries']),
    ('Training', 'Positive replay labels', ['success/positive_labels']),
    ('Training', 'Weighted self-imitation loss', ['success/weighted_loss']),
    ('Training', 'Value loss', ['ppo/final_epoch/value_loss']),
    ('Training', 'Policy KL', ['ppo/final_epoch/approximate_kl']),
    ('Training', 'Actor and critic epochs', ['ppo/actor_epochs', 'ppo/critic_epochs']),
    ('Evaluation (initial, every 30 minutes, final)', 'Validated area reduction (%)',
     ['evaluation/original/mean_area_reduction_percent', 'evaluation/overlap/mean_area_reduction_percent']),
    ('Evaluation (initial, every 30 minutes, final)', 'Validated best area',
     ['evaluation/original/mean_best_area', 'evaluation/overlap/mean_best_area']),
]
PANELS = [('Progress over training time', 'Area reduction (%) over training time',
           'evaluation/elapsed_seconds', ['evaluation/all/mean_area_reduction_percent',
            'evaluation/original/mean_area_reduction_percent', 'evaluation/overlap/mean_area_reduction_percent'])] + [
    (group, title, 'train/actions', keys) for group, title, keys in PANELS]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    existing = parser.add_mutually_exclusive_group()
    existing.add_argument('--verify-url', help='Verify an already saved view instead of creating another')
    existing.add_argument('--update-url', help='Replace panels in an existing duration view with this definition')
    parser.add_argument('--env-file', type=Path, default=Path(__file__).resolve().parents[2] / '.env')
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f'output already exists; use its workspace URL: {args.output}')
    load_environment(args.env_file)
    destination = tracking_destination(required=True)
    import wandb
    import wandb_workspaces.workspaces as ws
    import wandb_workspaces.reports.v2 as wr
    observer = read(args.run / 'wandb-observer.json')
    if observer['protocol_sha256'] != sha(args.run / 'protocol.json'):
        parser.error(f'observer protocol does not match {args.run}')
    remote = wandb.Api(timeout=10).run(f'{destination["entity"]}/{destination["project"]}/{observer["run_id"]}')
    required = {axis for _, _, axis, _ in PANELS} | {metric for _, _, _, keys in PANELS for metric in keys}
    present = set()
    for row in remote.scan_history(page_size=500):
        present.update(k for k in required if row.get(k) is not None)
        if present == required:
            break
    if present != required:
        parser.error(f'run {remote.id} has no history for {sorted(required - present)}')
    sections = []
    for group in dict.fromkeys(group for group, _, _, _ in PANELS):
        sections.append(ws.Section(name=group, is_open=True, panels=[
            wr.LinePlot(title=title, x=axis, y=keys,
                        title_x='Training elapsed seconds' if axis == 'evaluation/elapsed_seconds' else 'Training actions',
                        line_marks={key: 'points' for key in keys} if keys[0].startswith('evaluation/') else {})
            for section, title, axis, keys in PANELS if section == group]))
    workspace = ws.Workspace(**destination, name='Blueprint 64-action training', sections=sections,
        auto_generate_panels=False, runset_settings=ws.RunsetSettings(
            filters=f'Config("protocol_sha256") == "{observer["protocol_sha256"]}"',
            run_settings={remote.id: ws.RunSettings(disabled=False)}))
    if args.verify_url or args.update_url:
        workspace = ws.Workspace.from_url(args.verify_url or args.update_url)
        if (workspace.entity, workspace.project) != (destination['entity'], destination['project']):
            parser.error('workspace destination differs from the configured tracking destination')
        if args.update_url:
            if workspace.name != 'Blueprint 64-action training':
                parser.error('only the dedicated blueprint duration view can be updated')
            workspace.sections = sections
            workspace.save()
    else:
        workspace.save()
    restored = ws.Workspace.from_url(workspace.url)
    metric_name = lambda value: value if isinstance(value, str) else value.name
    actual = [(p.title, metric_name(p.x), [metric_name(k) for k in p.y]) for s in restored.sections for p in s.panels]
    expected = [(title, axis, keys) for _, title, axis, keys in PANELS]
    if actual != expected:
        raise RuntimeError(f'workspace readback differs: {workspace.url}')
    write(args.output, dict(schema='blueprint-duration-workspace-v2', url=workspace.url,
        run_id=remote.id, protocol_sha256=observer['protocol_sha256'], panels=PANELS,
        panel_readback_verified=True, all_metrics_present=True,
        wandb_workspaces_version=version('wandb-workspaces')))
    print(workspace.url)


if __name__ == '__main__':
    main()
