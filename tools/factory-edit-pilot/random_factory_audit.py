"""Audit fresh inputs, native witnesses, replay samples and optimizer continuity."""
import argparse
from collections import Counter
from copy import deepcopy
from pathlib import Path
import signal
import time

import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode, digest
from full_factory_input_learning import rollout
from full_factory_large_learning import unpack
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from random_factory_collection import check_sources, tensor_hash, update
from random_factory_generation import read, write, geometry_identity
from random_factory_pilot import SCHEMA, configure_torch, new_model, semantics


def require(condition, message):
    if not condition:
        raise ValueError(f'random-audit.identity: {message}')


def optimizer_hash(optimizer):
    state = optimizer.state_dict()
    tensors = [entry[key] for _, entry in sorted(state['state'].items()) for key in sorted(entry)]
    return digest(dict(groups=state['param_groups'], tensors=tensor_hash(tensors)))


def verify_result(runtime, result, scratch):
    path = Path(result['result_path'])
    require(sha(path) == result['result_sha256'], str(path))
    task_path = path.with_name('task.json')
    require(sha(task_path) == result['input_sha256'], str(task_path))
    task = read(task_path)
    require(task['identity'] == result['identity'] and task['policy_sha256'] == result['policy_sha256'], 'task identity')
    generation = result['generation']
    require(read(path.parent / 'generated/generation.json') == generation, 'generation record')
    require(generation['generation_seed'] == task['generation_seed'], 'generation seed')
    if result['status'] == 'generation-rejected':
        require(generation['status'] == 'rejected' and generation['admission_status'] != 'accepted', 'rejected admission')
        return
    require(result['status'] == 'complete' and generation['status'] == 'accepted', 'accepted episode')
    case, row = generation['case'], result['row']
    require(sha(case['request']) == generation['request_sha256'] and sha(case['initial']) == generation['initial_sha256'], 'generated inputs')
    for name, expected in case['files_sha256'].items():
        require(sha(Path(case['request']).parent / name) == expected, name)
    reference = read(Path(case['request']).with_name('reference.json'))
    require(reference['coverage']['factory_validated'], 'independent initial coverage')
    episode = unpack(Path(row['directory']) / 'episode.json.gz')
    best = unpack(Path(row['directory']) / 'best.json.gz')
    require(row['policy_sha256'] == result['policy_sha256'] == episode['policy_sha256'], 'episode policy')
    require(row['selected'] == task['horizon'] == len(episode['trace']) and row['budget_complete'] and episode['budget_complete'], 'episode completeness')
    require(episode['initial_metrics'] == row['initial_metrics'] == generation['initial_metrics'], 'initial score')
    require(episode['best_metrics'] == row['best_metrics'] and digest(best) == episode['best_sha256'], 'best identity')
    require(Counter(step['action']['kind'] for step in episode['trace']) == row['kinds'], 'action count')
    require(all(step.get('acceptance', {}).get('candidate_validated') for step in episode['trace']
                if step['status'] in ('improved', 'accepted_exploration', 'accepted_equal')), 'native acceptance records')
    initial = FactoryEpisode(runtime, case, 1, 120, 5000)
    require(initial.state['initial_metrics'] == row['initial_metrics'], 'native reset score')
    require(geometry_identity(initial.state['current']) == generation['geometry_sha256'], 'geometry identity')
    write(scratch / 'best.json', best)
    accepted = FactoryEpisode(runtime, dict(case, initial=str(scratch / 'best.json')), 1, 120, 5000)
    require(accepted.state['initial_metrics'] == row['best_metrics'], 'native best validity and score')
    if task['training']:
        require(sha(result['gradients']) == result['gradients_sha256'], 'gradient file')
        payload = torch.load(result['gradients'], map_location='cpu', weights_only=False)
        require(payload['policy_sha256'] == result['policy_sha256'] and payload['input_sha256'] == result['input_sha256'], 'gradient payload')
        gradients = payload['gradients']
        require(tensor_hash(gradients['log_probability_gradients'] + gradients['entropy_gradients']) == result['gradient_values_sha256'], 'gradient tensors')


def replay(runtime, protocol, result, directory, guard):
    task = read(Path(result['result_path']).with_name('task.json'))
    saved = torch.load(task['policy'], map_location='cpu', weights_only=False)
    model = new_model(protocol['spec'], 0)
    model.load_state_dict(saved['policy'], strict=True)
    require(parameter_hash(model) == task['policy_sha256'], 'replay policy')
    selection = dict(protocol['spec']['selection'], horizon=task['horizon'])
    row, gradients = rollout(runtime, dict(config=selection), result['generation']['case'], model, 'action-only',
                            task['action_seed'], directory, task['training'], guard)
    repeated = dict(result, row=row)
    if gradients is not None:
        repeated['gradient_values_sha256'] = tensor_hash(gradients['log_probability_gradients'] + gradients['entropy_gradients'])
    require(semantics(repeated) == semantics(result), f'replayed actions, observations, probabilities, native states and gradients: {directory}')
    return dict(result=result['result_path'], selected=row['selected'], product=result['generation']['draw']['product'],
                training=task['training'], native_and_sampling_match=True, gradients_match=gradients is not None)


def evaluation_curve(evaluations):
    initial = {r['identity']['index']: r for r in evaluations[0]['results']}
    points = []
    for evaluation in evaluations:
        counts = Counter()
        before_area, after_area = [], []
        for result in evaluation['results']:
            previous = initial[result['identity']['index']]
            require(result['status'] == previous['status'], 'paired evaluation admission')
            if result['status'] != 'complete':
                counts['generation_rejected'] += 1
                continue
            require(result['generation']['request_sha256'] == previous['generation']['request_sha256']
                    and result['generation']['initial_sha256'] == previous['generation']['initial_sha256'], 'paired evaluation starts')
            a, b = previous['row']['best_metrics']['score'], result['row']['best_metrics']['score']
            counts['wins' if b < a else 'losses' if b > a else 'ties'] += 1
            area = result['row']['initial_metrics']['score'][0]
            before_area.append(100 * (1 - a[0] / area))
            after_area.append(100 * (1 - b[0] / area))
        n = len(after_area)
        points.append(dict(update=evaluation['update'], **dict(counts), samples=n,
            initial_area_reduction_percent=sum(before_area) / n if n else None,
            area_reduction_percent=sum(after_area) / n if n else None,
            paired_area_delta_pp=(sum(after_area) - sum(before_area)) / n if n else None))
    return points


def run(args):
    require(0 < args.seconds <= 7200, 'audit time bound')
    configure_torch()
    protocol = read(args.screen / 'protocol.json')
    check_sources(protocol)
    require(not args.output.exists(), 'fresh audit output')
    args.output.mkdir()
    spec = protocol['spec']
    runtime = FactoryRuntime(spec['native_library'], protocol['catalog_file'])
    began = time.monotonic()
    verified, replays, learning = [], [], []
    def guard():
        if time.monotonic() - began > args.seconds:
            raise TimeoutError('random-audit.time-budget')
    try:
        scaling = read(args.screen / 'scaling/report.json')
        require(scaling['protocol_sha256'] == sha(args.screen / 'protocol.json'), 'scaling protocol')
        serial = next(cell for cell in scaling['cells'] if cell['workers'] == 1 and cell['status'] == 'complete')
        references = [read(row['path'])['signature'] for row in serial['repetitions']]
        for cell in scaling['cells']:
            if cell['status'] != 'complete':
                continue
            for index, row in enumerate(cell['repetitions']):
                guard()
                require(sha(row['path']) == row['sha256'], 'scaling record')
                record = read(row['path'])
                require(record['signature'] == references[index], 'serial-relative scaling parity')
                for result in record['results']:
                    guard()
                    verify_result(runtime, result, args.output)
                    verified.append(result['result_path'])
                model = new_model(spec, spec['policy']['seed'])
                optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
                accounting = update(model, optimizer, record['results'], spec['learning'])
                require(accounting == record['accounting'], 'scaling optimizer reconstruction')
                require(accounting['policy_after_sha256'] == record['signature']['policy_after']
                        and accounting['applied_gradient_sha256'] == record['signature']['gradients'], 'scaling update signature')
                sample = next(r for r in record['results'] if r['status'] == 'complete')
                replays.append(replay(runtime, protocol, sample, args.output / f'replay-scaling-{cell["workers"]}-{index}', guard))
        training = read(args.screen / 'learning/report.json')
        require(training['status'] == 'complete' and training['protocol_sha256'] == sha(args.screen / 'protocol.json'), 'learning report')
        for manifest in training['rows']:
            require(sha(manifest['path']) == manifest['sha256'], 'learning manifest')
            report = read(manifest['path'])
            require([g['group'] for g in report['groups']] == list(range(spec['learning']['groups'])), 'complete learning groups')
            require([e['update'] for e in report['evaluations']] == spec['learning']['evaluate_at'], 'scheduled evaluations')
            first = report['checkpoints'][0]
            require(sha(first['path']) == first['sha256'], 'initial checkpoint')
            saved = torch.load(first['path'], map_location='cpu', weights_only=False)
            require(saved['schema'] == SCHEMA and saved['protocol_sha256'] == sha(args.screen / 'protocol.json')
                    and saved['next_group'] == 0 and saved['seed'] == report['seed'], 'initial checkpoint metadata')
            require(len(report['checkpoints']) == len(report['groups']) + 1, 'checkpoint count')
            model = new_model(spec, report['seed'])
            require(parameter_hash(model) == saved['policy_sha256'], 'initial parameter seed')
            optimizer = torch.optim.Adam(model.parameters(), lr=spec['learning']['learning_rate'])
            optimizer.load_state_dict(saved['optimizer'])
            samples = {}
            for group in report['groups']:
                guard()
                for result in group['results']:
                    verify_result(runtime, result, args.output)
                    verified.append(result['result_path'])
                    if result['status'] == 'complete':
                        samples.setdefault(result['generation']['draw']['product'], result)
                accounting = update(model, optimizer, group['results'], spec['learning'])
                for key, value in accounting.items():
                    require(group[key] == value, f'optimizer reconstruction {report["seed"]}/{group["group"]}/{key}')
                after = group['checkpoint']
                require(sha(after['path']) == after['sha256'], 'group checkpoint file')
                saved = torch.load(after['path'], map_location='cpu', weights_only=False)
                require(saved['next_group'] == group['group'] + 1 and saved['seed'] == report['seed'], 'checkpoint position')
                require(saved['schema'] == SCHEMA and saved['protocol_sha256'] == sha(args.screen / 'protocol.json')
                        and after == report['checkpoints'][group['group'] + 1], 'checkpoint metadata')
                require(saved['policy_sha256'] == parameter_hash(model) == after['policy_sha256'], 'checkpoint policy hash')
                probe = new_model(spec, 0)
                probe.load_state_dict(saved['policy'], strict=True)
                require(parameter_hash(probe) == parameter_hash(model), 'checkpoint actual weights')
                other = torch.optim.Adam(probe.parameters(), lr=spec['learning']['learning_rate'])
                other.load_state_dict(saved['optimizer'])
                require(optimizer_hash(other) == optimizer_hash(optimizer), 'checkpoint Adam moments')
            for evaluation in report['evaluations']:
                for result in evaluation['results']:
                    guard()
                    verify_result(runtime, result, args.output)
                    verified.append(result['result_path'])
            for label, result in samples.items():
                replays.append(replay(runtime, protocol, result, args.output / f'replay-seed-{report["seed"]}-{label}', guard))
            for label, results in [('last-training', report['groups'][-1]['results']),
                                   ('endpoint', report['evaluations'][-1]['results'])]:
                result = next(r for r in results if r['status'] == 'complete')
                replays.append(replay(runtime, protocol, result, args.output / f'replay-seed-{report["seed"]}-{label}', guard))
            learning.append(dict(seed=report['seed'], curve=evaluation_curve(report['evaluations']),
                updates=len(report['groups']), nonflat_groups=sum(not g['flat_returns'] for g in report['groups']),
                training_episodes=sum(g['accepted'] for g in report['groups']),
                training_actions=sum(g['selected'] for g in report['groups'])))
            print(dict(stage='audited-seed', seed=report['seed'], verified=len(verified)), flush=True)
        check_sources(protocol)
        write(args.output / 'report.json', dict(status='complete', smoke=protocol['smoke'],
            protocol_sha256=sha(args.screen / 'protocol.json'), analysis_source_sha256=sha(__file__),
            scaling_sha256=sha(args.screen / 'scaling/report.json'), learning_sha256=sha(args.screen / 'learning/report.json'),
            verified_results=len(verified), replayed_actions=sum(r['selected'] for r in replays), replays=replays,
            learning=learning, seconds=time.monotonic() - began,
            scope='All recorded starts/bests independently reopened in native validation; all learning optimizer transitions reconstructed. Declared sample episodes re-executed from retained starts with exact observations, sampling, native states and training gradients.'))
        (args.output / 'best.json').unlink(missing_ok=True)
        print(dict(status='complete', results=len(verified), replayed=sum(r['selected'] for r in replays)), flush=True)
    except BaseException as error:
        write(args.output / 'partial.json', dict(status='incomplete', code='random-audit.failed', severity='error',
            stage='audit', entity=str(args.screen), message=str(error), verified=len(verified), replays=replays))
        raise
    finally:
        runtime.close()


if __name__ == '__main__':
    def interrupted(signum, _):
        raise InterruptedError(f'random-audit.interrupted: {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=3600)
    run(parser.parse_args())
