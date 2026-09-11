"""Audit paired lower-rate PPO outcomes against an immutable same-model control."""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import statistics
import time

from compare_ppo_combinations import check, read, sha, check_checkpoint_rate
from compare_ppo_capacity import load_audit, inputs, verified_read, runtime_controls, paired, compact
from ppo_spatial_model import shared_capacity_specification
from run_ppo_combinations import LOWER_RATES


TRACE_FIELDS = ('action', 'action_key', 'allowed_count', 'allowed_sha256',
    'observation_sha256', 'critic_observation_sha256', 'old_log_probability',
    'old_value', 'probabilities_sha256', 'sampling_entropy', 'current_before_sha256',
    'current_sha256', 'best_sha256', 'current_score', 'best_score', 'status',
    'area_reward', 'failure_cost', 'reward', 'reward_to_go', 'done')


def workload_controls(old, new, rate):
    a, b = deepcopy(old['config']), deepcopy(new['config'])
    check(a['model'] == b['model'] == shared_capacity_specification(True, True), 'fixed model differs')
    check(a['ppo'].pop('learning_rate') == 0.0003 and b['ppo'].pop('learning_rate') == rate,
          'declared learning rates differ')
    check(a == b, 'non-learning-rate configuration differs')
    result = runtime_controls(old, new, {'tools/factory-edit-pilot/ppo_harness_tracking.py'})
    return dict(result, only_learning_rate_configuration_differs=True)


def initial_collection(cell, identities, manifest):
    """Compare native outcomes and all behavior inputs before the first update."""
    root = Path(cell['root'])
    state = verified_read(root/'state.json', identities, manifest)
    result = {}
    steps = 0
    for job in state['jobs']:
        for phase in (dict(kind='evaluation', batch=0), dict(kind='training', batch=1)):
            refs = [r for r in job['records'] if r['phase'] == phase]
            check(len(refs) == 1, 'one initial evaluation and first collection required')
            record = read(root/refs[0]['path'], manifest, refs[0]['sha256'])
            for item in record['results']:
                key = (str(job['seed']), phase['kind'], item['identity']['index'])
                check(key not in result, 'duplicate initial collection position')
                detail = dict(status=item['status'], identity=item['identity'],
                              policy_sha256=item['policy_sha256'], critic_sha256=item['critic_sha256'])
                if item['status'] == 'complete':
                    episode = verified_read(Path(item['row']['directory'])/'episode.json.gz', identities, manifest)
                    # Some transitions omit optional observation fingerprints.
                    # Preserve field presence so missing and explicit values differ.
                    detail['trace'] = [{k: step[k] for k in TRACE_FIELDS if k in step} for step in episode['trace']]
                    detail['initial_metrics'] = episode['initial_metrics']
                    detail['best_metrics'] = episode['best_metrics']
                    steps += len(detail['trace'])
                result[key] = detail
    return result, steps


def optimization_details(cell, identities, manifest, rate):
    root = Path(cell['root'])
    protocol = verified_read(root/'protocol.json', identities, manifest)
    state = verified_read(root/'state.json', identities, manifest)
    result = {}
    for job in state['jobs']:
        rows, checkpoints = [], set()
        for ref in job['records']:
            record = read(root/ref['path'], manifest, ref['sha256'])
            checkpoint = record['checkpoint']
            path = (root/checkpoint['path']).resolve()
            if path not in checkpoints:
                check(identities.get(str(path)) == checkpoint['sha256'], 'checkpoint not covered by audited identities')
                check_checkpoint_rate(path.read_bytes(), rate, state['protocol_sha256'], job['id'], ref['phase']['batch'])
                checkpoints.add(path)
            if ref['phase']['kind'] != 'training':
                continue
            ppo = record['ppo']
            first, last = ppo['epochs'][0], ppo['epochs'][-1]
            rows.append(dict(batch=ref['phase']['batch'], actor_epochs=ppo['actor_epochs'],
                critic_epochs=ppo['critic_epochs'], first_epoch_kl=first['approximate_kl'],
                final_epoch_kl=last['approximate_kl'], first_epoch_outside_clip_fraction=first['outside_clip_fraction'],
                final_epoch_outside_clip_fraction=last['outside_clip_fraction'],
                final_epoch_objective_clipped_fraction=last['objective_clipped_fraction']))
        check(len(checkpoints) == protocol['config']['training']['batches'] + 1, 'complete checkpoint learning-rate coverage required')
        metrics = ('first_epoch_kl', 'final_epoch_kl', 'first_epoch_outside_clip_fraction',
                   'final_epoch_outside_clip_fraction', 'final_epoch_objective_clipped_fraction')
        summary = {f'mean_{key}': statistics.mean(r[key] for r in rows) for key in metrics}
        summary.update(max_first_epoch_kl=max(r['first_epoch_kl'] for r in rows),
                       first_epoch_above_target_batches=sum(r['first_epoch_kl'] > protocol['config']['ppo']['target_kl'] for r in rows))
        result[str(job['seed'])] = dict(summary=summary, by_batch=rows, checkpoint_learning_rates_verified=len(checkpoints))
    return result


def compare(provenance_path, lower_report_path, lower_inputs_path, output):
    check(not output.exists(), f'fresh comparison output required: {output}')
    began, manifest = time.monotonic(), {}
    provenance = read(provenance_path, manifest)
    check(provenance['schema'] == 'ppo-shared-capacity-final-provenance-v1', 'baseline provenance schema differs')
    old, old_ids = load_audit(Path(provenance['comparison']['path']), Path(provenance['comparison_inputs']['path']),
        manifest, 'ppo-shared-capacity-comparison-v1', provenance['comparison']['sha256'], provenance['comparison_inputs']['sha256'])
    new, new_ids = load_audit(lower_report_path, lower_inputs_path, manifest, 'ppo-learning-rate-comparison-v1')
    baseline = next(c for c in old['cells'] if c['name'] == 'shared-history-tail')
    check(len(new['cells']) == 2 and {c['name']: c['learning_rate'] for c in new['cells']} == LOWER_RATES,
          'both registered lower rates required')
    _, old_geometry, old_digest = inputs(old, old_ids, manifest)
    _, new_geometry, new_digest = inputs(new, new_ids, manifest)
    check(old_geometry == new_geometry and old_digest == new_digest, 'matched factory generation differs')
    for key in ('initial_actor_hashes', 'initial_critic_hashes'):
        check(old[key] == new[key], f'{key} differ')
    old_protocol = verified_read(Path(baseline['root'])/'protocol.json', old_ids, manifest)
    common, steps = initial_collection(baseline, old_ids, manifest)
    baseline_value = dict(compact(baseline), learning_rate=0.0003,
                          optimization=optimization_details(baseline, old_ids, manifest, 0.0003))
    cells, controls = [], []
    for cell in new['cells']:
        rate = cell['learning_rate']
        protocol = verified_read(Path(cell['root'])/'protocol.json', new_ids, manifest)
        controls.append(dict(learning_rate=rate, **workload_controls(old_protocol, protocol, rate)))
        collected, count = initial_collection(cell, new_ids, manifest)
        check(collected == common and count == steps, 'initial evaluation or first training collection differs')
        for seed in cell['jobs']:
            check(cell['jobs'][seed]['evaluations'][0] == baseline['jobs'][seed]['evaluations'][0],
                  'initial evaluation quality differs')
        cells.append(dict(compact(cell), learning_rate=rate, lower_minus_baseline=paired(cell, baseline),
                          optimization=optimization_details(cell, new_ids, manifest, rate)))
    report = dict(schema='ppo-learning-rate-controls-v1', status='complete', baseline=baseline_value, cells=cells,
        model=old_protocol['config']['model'], ppo_control=old_protocol['config']['ppo'],
        controls=controls, matched_input_identities=len(new_geometry), input_geometry_sha256=new_digest,
        initial_collection_positions=len(common), initial_collection_steps=steps,
        initial_collection_sha256=sha(json.dumps(sorted(common.items()), sort_keys=True, separators=(',', ':')).encode()),
        baseline_report_sha256=provenance['comparison']['sha256'], lower_report_sha256=manifest[str(lower_report_path.resolve())],
        source_sha256=sha(Path(__file__).read_bytes()), verified_file_identities=len(manifest), seconds=time.monotonic()-began,
        limitations=['Two run seeds and reused evaluation cohorts; exploratory conditional comparison.',
                     'Both actor and critic learning rates change together; their effects are not isolated.',
                     'Equal experience budgets do not imply equal optimizer work under KL stopping.',
                     'Critic errors use each policy trajectory, not common realized targets.',
                     'Retained controls were not run concurrently; wall-clock differences are descriptive.'])
    output.mkdir(parents=True)
    payload = (json.dumps(manifest, indent=2)+'\n').encode()
    report['input_manifest_sha256'] = sha(payload)
    (output/'inputs.json').write_bytes(payload)
    (output/'report.json').write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    print(dict(status='complete', lower_rates=len(cells), verified_files=len(manifest), seconds=report['seconds']), flush=True)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-provenance', type=Path, required=True)
    parser.add_argument('--lower-report', type=Path, required=True)
    parser.add_argument('--lower-inputs', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    compare(args.baseline_provenance.resolve(), args.lower_report.resolve(), args.lower_inputs.resolve(), args.output.resolve())
