"""Train or resume the registered blueprint PPO/BC comparison arm."""
import argparse
import json
from pathlib import Path
import time

import torch

from blueprint_ppo import check_sources, initialize as initialize_ppo
from blueprint_ppo_collect import collect
from blueprint_ppo_environment import Runtime, Stream
from blueprint_ppo_evaluate import evaluate
from blueprint_ppo_store import ROOT, checked, load, model_checkpoint, read, reference, restore_learners, save, sha, write
from blueprint_ppo_update import learners, require
from blueprint_bc_update import update_bc


def initialize(config_path, native_library, output, is_readiness, coefficient):
    config = read(config_path)
    require(config['bc']['schema'] == 'blueprint-bc-v1' and coefficient in config['bc']['coefficients'],
            'blueprint.bc-run: registered coefficient')
    dataset_path = ROOT / config['bc']['dataset']
    dataset = load(dataset_path)
    require(dataset['config_sha256'] == sha(config_path), 'blueprint.bc-run: data selection config identity')
    require(dataset['build_source_sha256'] == sha(Path(__file__).with_name('build_blueprint_bc_data.py')),
            'blueprint.bc-run: data builder identity')
    initialize_ppo(config_path, native_library, output, is_readiness)
    protocol = read(output / 'protocol.json')
    require(dataset['native_sha256'] == protocol['native_sha256'], 'blueprint.bc-run: native teacher identity')
    protocol['schema'] = 'blueprint-ppo-bc-protocol-v1'
    protocol['config']['bc']['coefficient'] = coefficient
    for path in (Path(__file__).resolve(), Path(__file__).with_name('blueprint_bc_update.py').resolve(),
                 Path(__file__).with_name('build_blueprint_bc_data.py').resolve(), dataset_path):
        protocol['sources'][str(path.relative_to(ROOT))] = sha(path)
    protocol['dataset_sha256'] = sha(dataset_path)
    write(output / 'protocol.json', protocol)
    state = read(output / 'state.json')
    state['protocol_sha256'] = sha(output / 'protocol.json')
    write(output / 'state.json', state)


def run(output):
    protocol = read(output / 'protocol.json')
    state = read(output / 'state.json')
    require(state['protocol_sha256'] == sha(output / 'protocol.json'), 'durable protocol identity')
    require(state['status'] != 'complete', 'experiment already completed')
    check_sources(protocol)
    config = protocol['config']
    dataset = load(ROOT / config['bc']['dataset'])
    require(dataset['schema'] == 'blueprint-bc-dataset-v1', 'blueprint.bc-run: dataset schema')
    demonstrations = [d for d in dataset['demonstrations'] if d['split'] == 'train']
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
                    result = update_bc(model, actor, critic, shuffle, payload['samples'], payload['frontiers'],
                                       config['ppo'], config['bc'], demonstrations, seed, ordinal)
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
    train.add_argument('--coefficient', type=float, required=True)
    train.add_argument('--readiness', action='store_true')
    resume = sub.add_parser('resume')
    resume.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    if args.command == 'train':
        initialize(args.config.resolve(), args.native_library.resolve(), args.output.resolve(), args.readiness, args.coefficient)
    run(args.output.resolve())


if __name__ == '__main__':
    main()
