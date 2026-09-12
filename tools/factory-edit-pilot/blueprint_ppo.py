"""Train or resume the registered persistent-draft blueprint PPO experiment."""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import platform
import shutil
import sys
import time

import torch

from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_evaluate import evaluate
from blueprint_ppo_store import ROOT, checked, load, model_checkpoint, read, reference, restore_learners, save, sha, write
from blueprint_ppo_update import digest, learners, require, update
from native import load_native
from blueprint_training_input import source_paths, validate_input


def validate(config):
    validate_input(config)
    require(config['schema'] == 'blueprint-persistent-ppo-v1' and config['device'] == 'cpu', 'registered CPU graph learner schema')
    training, evaluation, ppo = config['training'], config['evaluation'], config['ppo']
    require(ppo.get('advantage_normalization', 'standardize') in ('standardize', 'center'),
            'registered advantage normalization: standardize or center')
    require(type(ppo['width']) is int and ppo['width'] > 0
            and type(ppo.get('message_rounds', 2)) is int and ppo.get('message_rounds', 2) > 0,
            'positive integer graph width and message rounds')
    require(config['seeds'] and len(set(config['seeds'])) == len(config['seeds']), 'unique nonempty model seeds')
    require(training['updates'] >= 2 and 0 < training['chunk_actions'] < training['episode_actions'], 'persistent chunks shorter than real episodes')
    require(training['episode_actions'] % training['chunk_actions'] == 0 and
            training['updates'] % (training['episode_actions'] // training['chunk_actions']) == 0, 'complete terminal episodes at final update')
    names = [s['name'] for s in config['starts']]
    require(len(names) == len(set(names)) and training['streams'] > 0 and training['streams'] % len(names) == 0, 'balanced named starts')
    if config.get('duration', {}).get('schema') == 'blueprint-success-duration-v2':
        require(evaluation['interval_batches'] == 4 and 'at' not in evaluation,
                'duration evaluation uses every four batches, initialization and final stop')
        require(evaluation['cohort'] and evaluation['expected_cases'] == 32 and evaluation['elapsed_ms'] > 0,
                'duration evaluation requires the full historical 32-factory cohort')
    else:
        require(set(evaluation['starts']) <= set(names) and evaluation['samples'] > 0 and evaluation['elapsed_ms'] > 0, 'bounded known evaluation starts')
        require(evaluation['at'] == sorted(set(evaluation['at'])) and evaluation['at'][0] == 0
                and evaluation['at'][-1] == training['updates'], 'initial/final ordered evaluation checkpoints')
    require(ppo['learning_rate'] == .0003 and ppo['gamma'] == 1. and 0 < ppo['gae_lambda'] <= 1, 'fixed rate and continuing GAE')
    require(ppo['epochs'] == 4 and ppo['minibatch_size'] > 0 and ppo['target_kl'] > 0, 'bounded PPO epochs/minibatches/KL')


def readiness(config):
    config = deepcopy(config)
    config['purpose'] = 'readiness'
    config['seeds'] = [17]
    config['training'] = dict(updates=2, streams=4, chunk_actions=4, episode_actions=8, elapsed_ms=60000)
    config['evaluation'].update(at=[0, 2], samples=1, elapsed_ms=200, max_actions=64)
    config['ppo']['target_kl'] = 1e-9
    config['limits']['run_seconds'] = 120
    return config


def check_sources(protocol):
    for relative, expected in protocol['sources'].items():
        require(sha(ROOT / relative) == expected, f'frozen source/input changed: {relative}')
    require(sha(protocol['native_library']) == protocol['native_sha256'], 'frozen native library changed')
    for absolute, expected in protocol.get('external_sources', {}).items():
        require(sha(absolute) == expected, f'frozen external cohort input changed: {absolute}')


def initialize(config_path, native_library, output, is_readiness):
    require(not output.exists(), f'fresh experiment output required: {output}')
    config = read(config_path)
    if is_readiness:
        config = readiness(config)
    validate(config)
    if config['training_input']['kind'] == 'fresh-random' and config['training_input']['directory'] == '@run':
        config['training_input']['directory'] = str(output / 'training-inputs')
    module = load_native(native_library)
    build = json.loads(module.BlueprintEditSession.build_info_json())
    for relative, expected in build['files_sha256'].items():
        require(sha(ROOT / relative) == expected, f'native library is stale: {relative}')
    output.mkdir(parents=True)
    frozen_library = output / native_library.name
    shutil.copyfile(native_library, frozen_library)
    spec = read(ROOT / config['probe_spec'])
    python_files = ['blueprint_ppo.py', 'blueprint_ppo_store.py', 'blueprint_ppo_batch.py', 'blueprint_ppo_collect.py',
                    'blueprint_ppo_environment.py', 'blueprint_ppo_evaluate.py', 'blueprint_ppo_update.py',
                    'blueprint_edit_input.py', 'blueprint_edit_audit.py', 'probe_blueprint_edit.py', 'native.py',
                    'pyproject.toml', 'uv.lock']
    paths = [ROOT / 'tools/factory-edit-pilot' / name for name in python_files]
    paths += [config_path, ROOT / config['probe_spec'], *(ROOT / p for p in spec['catalogs'].values()),
              Path(__file__).with_name('blueprint_training_input.py'), *source_paths(config)]
    if config['training_input']['kind'] == 'fixed-fixture-diagnostic':
        paths.append(ROOT / spec['initial'])
    sources = dict(build['files_sha256'])
    sources.update({str(p.resolve().relative_to(ROOT)): sha(p) for p in paths})
    protocol = dict(schema='blueprint-ppo-protocol-v1', config=config, sources=sources,
                    native_library=str(frozen_library), native_sha256=sha(frozen_library), native_build=build,
                    runtime=dict(python=sys.version, torch=torch.__version__, platform=platform.platform()))
    write(output / 'protocol.json', protocol)
    write(output / 'state.json', dict(schema='blueprint-ppo-state-v1', status='prepared',
        protocol_sha256=sha(output / 'protocol.json'), jobs={}, seconds=0.))


def run(output):
    protocol = read(output / 'protocol.json')
    state = read(output / 'state.json')
    require(state['protocol_sha256'] == sha(output / 'protocol.json'), 'durable protocol identity')
    require(state['status'] != 'complete', 'experiment already completed')
    check_sources(protocol)
    config = protocol['config']
    runtime = Runtime(config, protocol['native_library'])
    began = time.perf_counter()
    previous_seconds = state['seconds']
    def guard():
        require(time.perf_counter() - began + previous_seconds < config['limits']['run_seconds'], 'experiment elapsed guard')
    state['status'] = 'running'
    write(output / 'state.json', state)
    for seed in config['seeds']:
        key = str(seed)
        job = state['jobs'].get(key)
        if job and job['status'] == 'complete':
            continue
        directory = output / f'seed-{seed}'
        if job is None:
            model, actor, critic = learners(seed, config['ppo'])
            sampler = torch.Generator().manual_seed(1_000_000 + seed)
            shuffle = torch.Generator().manual_seed(2_000_000 + seed)
            streams = [Stream(runtime, i) for i in range(config['training']['streams'])]
            checkpoint = directory / 'checkpoints/0000.pt.gz'
            save(checkpoint, model_checkpoint(model, actor, critic, sampler, shuffle, seed, 0, streams, state['protocol_sha256']))
            job = dict(status='running', update=0, latest_checkpoint=reference(checkpoint, output), records=[], evaluations=[], removed_checkpoints=[])
            state['jobs'][key] = job
            write(output / 'state.json', state)
        else:
            saved = load(checked(job['latest_checkpoint'], output))
            model, actor, critic, sampler, shuffle = restore_learners(saved, config['ppo'], state['protocol_sha256'])
            streams = [Stream.restore(runtime, s) for s in saved['streams']]
            require(saved['update'] == job['update'], 'checkpoint matches last durable update')
            job['status'] = 'running'
        partial = {}
        try:
            for ordinal in range(job['update'], config['training']['updates'] + 1):
                if ordinal > job['update']:
                    guard()
                    check_sources(protocol)
                    partial = {}
                    payload = collect(model, streams, sampler, config, guard, partial)
                    update_started = time.perf_counter()
                    result = update(model, actor, critic, shuffle, payload['samples'], payload['frontiers'], config['ppo'])
                    result['seconds'] = time.perf_counter() - update_started
                    payload.update(seed=seed, update=ordinal, protocol_sha256=state['protocol_sha256'], ppo=result)
                    chunk_path = directory / f'chunks/{ordinal:04d}.pt.gz'
                    save(chunk_path, payload)
                    checkpoint = directory / f'checkpoints/{ordinal:04d}.pt.gz'
                    save(checkpoint, model_checkpoint(model, actor, critic, sampler, shuffle, seed, ordinal, streams, state['protocol_sha256']))
                    record = dict(seed=seed, update=ordinal, chunk=reference(chunk_path, output), checkpoint=reference(checkpoint, output),
                        actions=len(payload['samples']), area_reward=sum(s['reward'] for s in payload['samples']),
                        improved_commits=sum(s['outcome']['status'] == 'improved' for s in payload['samples']),
                        valid_commits=sum(s['validated_witness'] is not None for s in payload['samples']),
                        completed_episodes=len(payload['completed']), continuity_checks=sum(s.boundaries for s in streams),
                        collection_seconds=payload['collection_seconds'], ppo=result)
                    record_path = directory / f'updates/{ordinal:04d}.json'
                    write(record_path, record)
                    job['records'].append(reference(record_path, output))
                    job.update(update=ordinal, latest_checkpoint=reference(checkpoint, output))
                    state['seconds'] = previous_seconds + time.perf_counter() - began
                    write(output / 'state.json', state)
                    print(json.dumps(dict(seed=seed, update=ordinal, actions=record['actions'], reward=record['area_reward'],
                        improvements=record['improved_commits'], actor_epochs=result['actor_epochs'],
                        bootstraps=result['nonterminal_bootstraps'], seconds=round(state['seconds'], 2))), flush=True)
                if ordinal in config['evaluation']['at'] and not any(e['update'] == ordinal for e in job['evaluations']):
                    eval_directory = directory / f'evaluation/{ordinal:04d}'
                    rows = evaluate(runtime, model, seed, ordinal, eval_directory, guard)
                    eval_path = eval_directory / 'report.json'
                    write(eval_path, dict(schema='blueprint-ppo-evaluation-v1', seed=seed, update=ordinal, rows=rows,
                                         cases=[reference(p, output) for p in sorted(eval_directory.glob('*.pt.gz'))]))
                    job['evaluations'].append(dict(update=ordinal, **reference(eval_path, output)))
                    state['seconds'] = previous_seconds + time.perf_counter() - began
                    write(output / 'state.json', state)
                    print(json.dumps(dict(seed=seed, evaluation=ordinal,
                        policy_area_reduction_percent=sum(r['policy_area_reduction_percent'] for r in rows) / len(rows),
                        mean_actions=sum(r['policy_actions'] for r in rows) / len(rows))), flush=True)
                guard()
                require(sum(p.stat().st_size for p in output.rglob('*') if p.is_file()) < config['limits']['max_output_bytes'], 'experiment output byte guard')
            require(all(s.session is None for s in streams), 'final update completes every registered real episode')
            job['status'] = 'complete'
            # Keep actual native snapshots at selected checkpoints plus every
            # non-replayable chunk input. Other optimizer snapshots are regenerable.
            retained = {0, 1, 2, *config['evaluation']['at'], config['training']['updates']}
            for path in sorted((directory / 'checkpoints').glob('*.pt.gz')):
                if int(path.name.split('.')[0]) not in retained:
                    job['removed_checkpoints'].append(reference(path, output))
                    path.unlink()
            state['seconds'] = previous_seconds + time.perf_counter() - began
            write(output / 'state.json', state)
        except BaseException as error:
            save(directory / 'interrupted.pt.gz', dict(schema='blueprint-ppo-interrupted-v1', error=str(error), partial=partial,
                current=model_checkpoint(model, actor, critic, sampler, shuffle, seed, job['update'], streams, state['protocol_sha256'])))
            job['status'] = 'failed'
            state.update(status='failed', seconds=previous_seconds + time.perf_counter() - began, error=str(error))
            write(output / 'state.json', state)
            raise
    check_sources(protocol)
    state.update(status='complete', seconds=previous_seconds + time.perf_counter() - began)
    write(output / 'state.json', state)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    train = sub.add_parser('train')
    train.add_argument('--config', type=Path, required=True)
    train.add_argument('--native-library', type=Path, required=True)
    train.add_argument('--output', type=Path, required=True)
    train.add_argument('--readiness', action='store_true')
    resume = sub.add_parser('resume')
    resume.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    if args.command == 'train':
        initialize(args.config.resolve(), args.native_library.resolve(), args.output.resolve(), args.readiness)
    run(args.output.resolve())


if __name__ == '__main__':
    main()
