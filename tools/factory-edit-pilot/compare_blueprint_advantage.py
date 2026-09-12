"""Compare registered fresh-input arms, including common immutable policy probes."""
import argparse
from collections import Counter
from copy import deepcopy
from pathlib import Path
from statistics import mean

import torch

from blueprint_duration_metrics import evaluation_metrics
from blueprint_edit_input import prepare
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_store import checked, load, read, reference, sha, write
from blueprint_ppo_update import digest, learners, require
from compare_blueprint_capacity import compare_rows
from summarize_blueprint_inputs import summarize


def nonfactor_config(config):
    result = deepcopy(config)
    for key in ('purpose', 'seeds', 'experiment'):
        result.pop(key)
    result['ppo'].pop('advantage_normalization')
    return result


def fixed_probes(output, matrix):
    arm = next(a for a in matrix['arms'] if a['variant'] == 'standardize' and a['seed'] == 17)
    directory = output / arm['directory']
    job = read(directory / 'state.json')['jobs']['17']
    report = read(checked(job['evaluations'][0], directory))
    require(report['update'] == 0, 'fixed policy probes must come from the untrained evaluation')
    probes = [prepare(load(checked(ref, directory))['initial']) for ref in report['cases']]
    return probes, dict(arm=arm['name'], cases=report['cases'], digest=digest(probes))


def probe_probability(model, probes):
    probabilities = []
    with torch.no_grad():
        for probe in probes:
            batch = pack([probe])
            logits, _ = model.forward_packed(batch)
            logp, _ = distribution(logits, batch)
            indices = [i for i, action in enumerate(probe['actions']) if action['kind'] == 'pose']
            probabilities.append(float(logp[indices].exp().sum()))
    return 100 * mean(probabilities)


def critic_return_error(directory, report):
    errors = []
    for ref in report['cases']:
        case = load(checked(ref, directory))
        future = 0.
        for entry in reversed(case['trace']):
            future += entry['outcome']['reward']
            errors.append(entry['old_value'] - future)
    require(bool(errors), 'critic return comparison needs actual evaluation trajectories')
    return dict(mae=mean(abs(e) for e in errors), bias=mean(errors), samples=len(errors))


def analyze(output):
    matrix, state = read(output / 'matrix.json'), read(output / 'state.json')
    require(matrix['schema'] == 'blueprint-advantage-protocol-v1' and state['status'] == 'complete'
            and state['matrix_sha256'] == sha(output / 'matrix.json'), 'completed source-bound advantage matrix required')
    spec = matrix['spec']
    require({(a['variant'], a['seed']) for a in matrix['arms']} ==
            {(v, s) for v in spec['variants'] for s in spec['seeds']} and len(matrix['arms']) == 6,
            'exact three-seed two-variant arm coverage')
    probes, probe_record = fixed_probes(output, matrix)
    shared, inputs_digest, arms, curves, training, finals = None, None, [], [], [], {}
    initial_models = {}
    for arm in matrix['arms']:
        directory, seed = output / arm['directory'], arm['seed']
        protocol = read(checked(arm['protocol'], output))
        config = protocol['config']
        require(config['seeds'] == [seed] and config['training']['updates'] == spec['updates'], 'matched learner seed/action budget')
        require(config['ppo']['advantage_normalization'] == arm['variant'] and
                (config['ppo']['width'], config['ppo']['message_rounds']) == (40, 2), 'registered advantage factor and architecture')
        require(config['experiment']['matrix_sha256'] == matrix['matrix_sha256'], 'registered matrix identity')
        bound_config = [k for k in protocol['sources'] if Path(k).name == f'{arm["name"]}.json' and Path(k).parent.name == 'configs']
        require(len(bound_config) == 1 and sha(output / 'configs' / f'{arm["name"]}.json') == protocol['sources'][bound_config[0]],
                'bound generated arm configuration')
        normalized = dict(config=nonfactor_config(config), native_sha256=protocol['native_sha256'], runtime=protocol['runtime'],
            external_sources=protocol['external_sources'], native_build=protocol['native_build'],
            sources={k: v for k, v in protocol['sources'].items() if k != bound_config[0]})
        if shared is None:
            shared = normalized
        require(shared == normalized, 'advantage comparison changed a non-factor setting/source/runtime')
        parent = state['arms'][arm['name']]
        require(parent['status'] == 'complete', 'complete matrix child')
        completion, child = (read(checked(parent[k], output)) for k in ('completion', 'state'))
        require(completion['status'] == child['status'] == 'complete' and child['protocol_sha256'] == arm['protocol']['sha256'],
                'completed source-bound child')
        checked(completion['state'], directory)
        audit = read(checked(completion['tail_audit'], directory))
        require(audit['status'] == 'passed' and audit['protocol_sha256'] == child['protocol_sha256'], 'passing final learner audit')
        observer = read(checked(parent['observer'], output))
        require(observer['status'] == 'finished' and observer['protocol_sha256'] == child['protocol_sha256'], 'finished tracking')
        job = child['jobs'][str(seed)]
        require(job['update'] == len(job['records']) == spec['updates'], 'full registered training budget')
        init = job['initialization']
        require(init['evaluated_before_training'] and init['imported_checkpoint'] is None and
                init['actor_optimizer_entries'] == init['critic_optimizer_entries'] == init['success_sequences'] == 0,
                'fresh learner, optimizers and success buffer')
        require(seed not in initial_models or initial_models[seed] == init['model_sha256'], 'paired fresh initial weights')
        initial_models[seed] = init['model_sha256']
        supplied = summarize(directory)['jobs'][0]
        if inputs_digest is None:
            inputs_digest = supplied['ordered_input_digest']
        require(inputs_digest == supplied['ordered_input_digest'] and supplied['completed_episodes'] ==
                spec['updates'] * config['training']['streams'] // 2, 'exact same fresh input sequence and complete episode budget')
        reports = [read(checked(ref, directory)) for ref in job['evaluations']]
        require([r['update'] for r in reports] == list(range(0, spec['updates'] + 1, 4)), 'registered evaluation cadence')
        require(all(len(r['rows']) == 32 for r in reports), 'complete historical evaluation cohort')
        model, _, _ = learners(seed, config['ppo'])
        require(digest(model.state_dict()) == init['model_sha256'], 'recorded cold initialization matches the registered seed')
        points = []
        for report, ref in zip(reports, job['evaluations'], strict=True):
            values = evaluation_metrics(report['rows'], reports[0]['rows'])
            checkpoint_ref = job['latest_checkpoint'] if report['update'] == spec['updates'] else (
                read(checked(job['records'][report['update'] - 1], directory))['checkpoint'] if report['update'] else
                reference(directory / f'seed-{seed}/checkpoints/0000.pt.gz', directory))
            checkpoint = load(checked(checkpoint_ref, directory))['learner']
            require(checkpoint['update'] == report['update'] and checkpoint['protocol_sha256'] == child['protocol_sha256'],
                    'source-bound probe checkpoint')
            model.load_state_dict(checkpoint['model'])
            require(digest(model.state_dict()) == checkpoint['model_sha256'], 'probe model identity')
            if report['update'] == 0:
                require(checkpoint['model_sha256'] == init['model_sha256'], 'untrained probe checkpoint identity')
            point = dict(arm=arm['name'], variant=arm['variant'], seed=seed, update=report['update'], seconds=ref['seconds'],
                area_reduction_percent=values['evaluation/all_admitted/by_batches/area_reduction_percent'],
                delta_from_initial_pp=values['evaluation/all_admitted/by_batches/delta_from_initial_pp'],
                fixed_pose_probability_percent=probe_probability(model, probes))
            points.append(point)
        curves.extend(points)
        records = [read(checked(ref, directory)) for ref in job['records']]
        for row in records:
            require(row['ppo']['advantage_normalization'] == arm['variant'], 'actual optimizer normalization mode')
            training.append(dict(arm=arm['name'], update=row['update'],
                **{k: row['ppo'][k] for k in ('raw_advantage_std', 'actor_advantage_std', 'advantage_scale', 'zero_reward_batch')},
                critic_target_mae=row['ppo']['epochs'][-1]['value_mae']))
        actions = sum(r['actions'] for r in records)
        require(actions == spec['updates'] * config['training']['streams'] * config['training']['chunk_actions'], 'matched actual training actions')
        window = [p for p in points if p['update'] > spec['updates'] - spec['last_window_batches']]
        all_cases = [r for report in reports for r in report['rows']]
        train_window = records[-spec['last_window_batches']:]
        finished = sum(r['quality'].get('accepted', 0) for r in train_window)
        summary = {k: v for k, v in supplied.items() if k != 'input_records'}
        arms.append(dict(name=arm['name'], variant=arm['variant'], seed=seed, cpu=parent['cpu'],
            training_actions=actions, input_coverage=summary, initial_area_reduction_percent=points[0]['area_reduction_percent'],
            final_area_reduction_percent=points[-1]['area_reduction_percent'], delta_from_initial_pp=points[-1]['delta_from_initial_pp'],
            last_window_mean_percent=mean(p['area_reduction_percent'] for p in window),
            last_window_training_mean_percent=sum(r['quality'].get('accepted', 0) *
                r['quality'].get('mean_area_reduction_percent', 0.) for r in train_window) / finished,
            initial_fixed_pose_probability_percent=points[0]['fixed_pose_probability_percent'],
            final_fixed_pose_probability_percent=points[-1]['fixed_pose_probability_percent'],
            zero_reward_batches=sum(r['ppo']['zero_reward_batch'] for r in records),
            positive_commits=records[-1]['buffer']['positive_commits'], buffer_entries=records[-1]['buffer']['entries'],
            final_evaluation_counts=dict(sum((Counter(r['counts']) for r in reports[-1]['rows']), Counter())),
            final_critic_return_error=critic_return_error(directory, reports[-1]),
            last_window_critic_target_mae=mean(r['ppo']['epochs'][-1]['value_mae'] for r in train_window),
            active_seconds=child['seconds'], optimizer_seconds=sum(r['ppo']['seconds'] for r in records),
            collection_seconds=sum(r['collection_seconds'] for r in records),
            actual_input_access_seconds=sum(r['quality'].get('input_access_seconds', 0.) for r in records),
            evaluation_cases=len(all_cases), evaluation_guards=dict(Counter(r['guard'] for r in all_cases)),
            full_action_cases=sum(r['policy_actions'] == 64 for r in all_cases),
            url=observer['url'], protocol=arm['protocol'], completion=parent['completion'], audit=parent['audit']))
        finals[(arm['variant'], seed)] = reports[-1]['rows']
    require(digest(probes) == probe_record['digest'], 'immutable shared probes changed during analysis')
    for arm in arms:
        arm['paired_final_vs_standardize'] = compare_rows(finals[(arm['variant'], arm['seed'])], finals[('standardize', arm['seed'])])
    aggregates = []
    for variant in spec['variants']:
        chosen = [a for a in arms if a['variant'] == variant]
        row = dict(variant=variant)
        for key in ('initial_area_reduction_percent', 'final_area_reduction_percent', 'delta_from_initial_pp',
                    'last_window_mean_percent', 'last_window_training_mean_percent', 'final_fixed_pose_probability_percent',
                    'last_window_critic_target_mae', 'active_seconds'):
            row[key] = mean(a[key] for a in chosen)
        row['paired_final_vs_standardize'] = {k: sum(a['paired_final_vs_standardize'][k] for a in chosen) for k in ('win', 'tie', 'loss')}
        aggregates.append(row)
    return dict(schema='blueprint-advantage-analysis-v1', matrix=reference(output / 'matrix.json', output),
        state=reference(output / 'state.json', output), spec=spec, arms=arms, aggregates=aggregates, curves=curves, training=training,
        fixed_probes=probe_record, input_sequence_sha256=inputs_digest,
        limitations=['Fixed action-budget screen; convergence and long-run superiority are untested.',
                     'Three learner seeds share one actual random training-input sequence.',
                     'Evaluation uses 32 fixed factories and one sampling seed per factory.',
                     'Elapsed time remains a policy feature; actual guards and CPU assignments are reported.',
                     'Fixed-probe movement probability is a diagnostic, not the optimization objective.',
                     'Critic return error uses sampled held-out trajectories, not exact policy values.'])


def plot(result, destination):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    figure, axes = plt.subplots(1, 2, figsize=(13, 4.5), constrained_layout=True)
    colors, styles = {'standardize': '#2563eb', 'center': '#dc2626'}, {17: '-', 29: '--', 41: ':'}
    for axis, key, label in zip(axes, ('area_reduction_percent', 'fixed_pose_probability_percent'),
                               ('Mean validated area reduction (%)', 'Pose probability on identical fixed inputs (%)')):
        for arm in result['arms']:
            points = [p for p in result['curves'] if p['arm'] == arm['name']]
            axis.plot([p['update'] for p in points], [p[key] for p in points], color=colors[arm['variant']],
                      linestyle=styles[arm['seed']], linewidth=1.2, alpha=.85, label=arm['name'])
        axis.set(xlabel='Optimizer updates (256 training actions each)', ylabel=label, ylim=(0, None))
        axis.grid(alpha=.2)
    axes[1].legend(fontsize=8)
    figure.suptitle('Fresh random factory training: advantage normalization, three learner seeds')
    figure.savefig(destination, dpi=180)
    plt.close(figure)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--plot', type=Path)
    args = parser.parse_args()
    require(not args.output.exists(), f'new analysis output required: {args.output}')
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    result = analyze(args.matrix.resolve())
    result['analysis_source_sha256'] = sha(Path(__file__))
    write(args.output, result)
    if args.plot:
        plot(result, args.plot)
    for row in result['aggregates']:
        print(row)
