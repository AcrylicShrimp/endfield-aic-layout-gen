"""Profile the unchanged full-factory ownership boundary on retained PPO episodes."""
import argparse
import cProfile
import gzip
import hashlib
import json
from pathlib import Path
import platform
import shutil
import statistics
import time

import torch

from direct_area_audit import physics_signature
from full_factory_environment import FactoryRuntime
from full_factory_input_contracts import observation_hash
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha
from ppo_area_model import models
from ppo_area_update import load_experience, save_experience
from ppo_training_rollout import rollout
from profile_factory_ownership_support import Timings, instrument, python_summary, node_summary
from random_factory_generation import generate, read, write


def require(condition, message):
    if not condition:
        raise ValueError(f'ownership-profile.contract: {message}')


def select(record):
    accepted = [r for r in record['results'] if r['status'] == 'complete']
    order = lambda r: (r['row']['initial_metrics']['score'][0], r['identity']['index'])
    selected = []
    for product in sorted({r['generation']['draw']['product'] for r in accepted}):
        rows = sorted((r for r in accepted if r['generation']['draw']['product'] == product), key=order)
        selected.append((f'{product}-median-area', rows[(len(rows)-1)//2]))
    largest = max(accepted, key=order)
    if not any(r['identity'] == largest['identity'] for _, r in selected):
        selected.append(('largest-start-area', largest))
    return selected


def node_runtime(protocol):
    return FactoryRuntime(protocol['spec']['native_library'], protocol['catalog_file'])


def clear_descriptions(runtime):
    # Remove only cross-replay warmth; normal within-episode cache behavior remains.
    runtime.description_cache.clear()
    runtime.description_cache_bytes = 0


def collect(runtime, protocol, sample, actor, critic, output, guard):
    started = time.perf_counter()
    row, payload = rollout(runtime, dict(config=protocol['spec']['selection']), sample['generation']['case'],
                           actor, critic, 'current-history-16', sample['identity']['action_seed'],
                           output, True, guard)
    episode_seconds = time.perf_counter() - started
    started = time.perf_counter()
    payload.update(policy_sha256=row['policy_sha256'], input_sha256=sample['input_sha256'])
    save_experience(output / 'experience.pt.gz', payload)
    file_hash = sha(output / 'experience.pt.gz')
    persistence_seconds = time.perf_counter() - started
    return row, payload, dict(episode_seconds=episode_seconds, experience_persistence_seconds=persistence_seconds,
                              total_seconds=episode_seconds+persistence_seconds, experience_sha256=file_hash,
                              experience_bytes=(output/'experience.pt.gz').stat().st_size)


def verify(sample, expected, row, payload):
    require(physics_signature(dict(sample, row=row)) == physics_signature(sample), 'physical/sampling trace differs')
    require(len(payload['steps']) == len(expected['steps']), 'episode length differs')
    for ordinal, (a, b) in enumerate(zip(payload['steps'], expected['steps'])):
        require(observation_hash(a) == observation_hash(b), f'actor/critic experience differs at {ordinal}')
    return dict(steps=len(payload['steps']), physical_sampling_and_observation_parity=True)


def run(args):
    require(not args.output.exists(), f'output already exists: {args.output}')
    started = time.monotonic()
    def guard():
        if time.monotonic() - started > args.seconds:
            raise TimeoutError(f'ownership-profile.budget: {args.seconds} seconds')
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    protocol = read(args.screen/'protocol.json')
    job = args.screen/'current-history-16--seed-17'/'training-0004'
    records = list(job.glob('record-*.json'))
    require(len(records) == 1, f'expected one committed record: {job}')
    record = read(records[0])
    selected = select(record)
    checkpoint = args.screen/record['behavior_checkpoint']['path']
    require(sha(checkpoint) == record['behavior_checkpoint']['sha256'], 'behavior checkpoint file differs')
    setup_began = time.perf_counter()
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    actor, critic = models(protocol['spec'], 0)
    actor.load_state_dict(saved['policy']); critic.load_state_dict(saved['critic'])
    require(parameter_hash(actor) == record['behavior_checkpoint']['policy_sha256'], 'actor weights differ')
    require(parameter_hash(critic) == record['behavior_checkpoint']['critic_sha256'], 'critic weights differ')
    setup_seconds = time.perf_counter() - setup_began
    args.output.mkdir(parents=True)
    sources = {str(p.relative_to(Path(__file__).resolve().parents[2])): sha(p)
               for p in Path(__file__).parent.glob('*.py')}
    sources.update({str(p.relative_to(Path(__file__).resolve().parents[2])): sha(p)
                    for p in Path(__file__).parent.glob('*.mjs')})
    report = dict(schema='full-factory-ownership-profile-v1', status='running', candidate='current-history-16',
        protocol_sha256=sha(args.screen/'protocol.json'), source_sha256=sha(__file__), sources=sources,
        checkpoint=dict(path=str(checkpoint), sha256=sha(checkpoint)), checkpoint_setup_seconds=setup_seconds,
        native_library=dict(path=protocol['spec']['native_library'], sha256=sha(protocol['spec']['native_library'])),
        hardware=dict(platform=platform.platform(), python=platform.python_version(), torch=torch.__version__, threads=1),
        repetitions=args.repeats, warmups=1, selected=[dict(label=label, identity=r['identity'],
          product=r['generation']['draw']['product'], facilities=r['row']['instances'],
          initial_score=r['row']['initial_metrics']['score']) for label, r in selected], cases=[],
        interpretation='Serial per-episode replay profiles. Instrumented times are diagnostic, not throughput. No PPO update occurs. Uninstrumented repetitions clear cross-replay description cache and exclude checkpoint loading/runtime open/parity checks. Generation repeats the retained draw, outside training.')
    write(args.output/'report.json', report)
    # This selection/protocol is written before observing any profile result.
    for label, sample in selected:
        guard()
        output = args.output/label
        output.mkdir()
        write(output/'reference-result.json', sample)
        original = load_experience(sample)
        trials, generations = [], []
        opened = time.perf_counter()
        runtime = node_runtime(protocol)
        runtime_open_seconds = time.perf_counter() - opened
        try:
            for index in range(args.repeats+1):
                guard()
                clear_descriptions(runtime)
                directory = output/f'collection-{index}'
                row, payload, timing = collect(runtime, protocol, sample, actor, critic, directory, guard)
                parity = verify(sample, original, row, payload)
                if index:
                    trials.append(timing)
                # Retain one complete repeat; other identical repeat payloads are regenerable.
                if index != 1:
                    shutil.rmtree(directory)
                print(json.dumps(dict(stage='collection', case=label, repeat=index, seconds=timing['total_seconds'], parity=parity)), flush=True)
            for index in range(args.repeats+1):
                guard()
                clear_descriptions(runtime)
                directory = output/f'generation-{index}'
                _, generation = generate(runtime, protocol['spec'], sample['identity']['generation_seed'], directory, guard)
                require(generation['status'] == 'accepted' and all(generation[k] == sample['generation'][k]
                    for k in ('request_sha256','initial_sha256','geometry_sha256')), f'fresh generation replay differs: {label}')
                if index:
                    generations.append({k:generation[k] for k in ('seconds','admission_seconds','setup_seconds')})
                    generations[-1]['constructor'] = {k:generation['original_attempt'][k] for k in ('flow_ms','construction_ms','validation_ms')}
                if index != 1:
                    shutil.rmtree(directory)
                print(json.dumps(dict(stage='generation', case=label, repeat=index, seconds=generation['seconds'])), flush=True)
        finally:
            runtime.close()
        measures = Timings()
        profile = cProfile.Profile()
        with instrument(measures, output/'node-collection', actor, critic):
            runtime = node_runtime(protocol)
            try:
                with profile, measures.region('collection.total'):
                    row, payload, profiled = collect(runtime, protocol, sample, actor, critic, output/'collection-profiled', guard)
            finally:
                runtime.close()
        parity = verify(sample, original, row, payload)
        profile.dump_stats(str(output/'collection.pstats'))
        generation_measures, generation_profile = Timings(), cProfile.Profile()
        with instrument(generation_measures, output/'node-generation'):
            runtime = node_runtime(protocol)
            try:
                with generation_profile, generation_measures.region('generation.total'):
                    _, generation = generate(runtime, protocol['spec'], sample['identity']['generation_seed'], output/'generation-profiled', guard)
            finally:
                runtime.close()
        require(generation['status'] == 'accepted' and all(generation[k] == sample['generation'][k]
            for k in ('request_sha256','initial_sha256','geometry_sha256')), 'instrumented generation differs')
        generation_profile.dump_stats(str(output/'generation.pstats'))
        result = dict(label=label, runtime_open_seconds=runtime_open_seconds, collection_trials=trials,
            generation_trials=generations, collection_median_seconds=statistics.median(r['total_seconds'] for r in trials),
            generation_median_seconds=statistics.median(r['seconds'] for r in generations), parity=parity,
            collection_profile=dict(**measures.report(), python=python_summary(profile), node=node_summary(output/'node-collection')),
            generation_profile=dict(**generation_measures.report(), python=python_summary(generation_profile), node=node_summary(output/'node-generation')))
        report['cases'].append(result)
        write(args.output/'report.json', report)
        print(json.dumps(dict(stage='case-complete', case=label, collection=result['collection_median_seconds'], generation=result['generation_median_seconds'])), flush=True)
    report.update(status='complete', seconds=time.monotonic()-started)
    write(args.output/'report.json', report)
    print(json.dumps(dict(status='complete', cases=len(selected), seconds=report['seconds'])), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--seconds', type=float, default=1200)
    args = parser.parse_args()
    if not 1 <= args.repeats <= 11 or not 0 < args.seconds <= 1800:
        parser.error('repeats must be 1..11 and seconds must be in (0, 1800]')
    preexisting = args.output.exists()
    try:
        run(args)
    except BaseException as error:
        if not preexisting and (args.output/'report.json').is_file():
            value = read(args.output/'report.json')
            value.update(status='failed', diagnostic=dict(code='ownership-profile.failed', severity='error',
              stage='profiling', entity=str(args.screen), message=str(error)))
            write(args.output/'report.json', value)
        raise
