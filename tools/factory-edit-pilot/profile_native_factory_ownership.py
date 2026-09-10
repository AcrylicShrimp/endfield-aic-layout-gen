"""Second ownership profile: current native runtime against frozen behavior evidence."""
import argparse
from contextlib import ExitStack
import cProfile
from copy import deepcopy
import json
from pathlib import Path
import shutil
import statistics
import time
from unittest.mock import patch

import torch

from native_factory_runtime import FactoryRuntime, ROOT
from ppo_area_model import models
from ppo_area_update import load_experience
from profile_factory_ownership import collect, verify, select, require
from profile_factory_ownership_support import Timings, python_summary
from random_factory_generation import generate, read, write
from full_factory_on_policy import parameter_hash
from full_factory_screen import sha


def current_spec(protocol):
    spec = deepcopy(protocol['spec'])
    previous_root = Path(spec['cli']).parents[2]
    for key in ('native_library', 'cli', 'validator'):
        spec[key] = str(ROOT / Path(spec[key]).relative_to(previous_root))
    spec['catalogs'] = {key: str(ROOT / Path(value).relative_to(previous_root)) for key, value in spec['catalogs'].items()}
    for product in spec['products']:
        product['source'] = str(ROOT / Path(product['source']).relative_to(previous_root))
    return spec


def run(args):
    require(not args.output.exists(), f'output exists: {args.output}')
    began = time.monotonic()
    def guard():
        require(time.monotonic() - began < args.seconds, 'profile wall-clock budget exhausted')
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    protocol = read(args.screen / 'protocol.json')
    record = read(next((args.screen / 'current-history-16--seed-17/training-0004').glob('record-*.json')))
    checkpoint = args.screen / record['behavior_checkpoint']['path']
    require(sha(checkpoint) == record['behavior_checkpoint']['sha256'], 'reference checkpoint hash')
    actor, critic = models(protocol['spec'], 0)
    saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
    actor.load_state_dict(saved['policy'])
    critic.load_state_dict(saved['critic'])
    require(parameter_hash(actor) == record['behavior_checkpoint']['policy_sha256'], 'actor identity')
    require(parameter_hash(critic) == record['behavior_checkpoint']['critic_sha256'], 'critic identity')
    protocol['spec'] = current_spec(protocol)
    args.output.mkdir(parents=True)
    catalog_file = args.output / 'catalogs.json'
    write(catalog_file, protocol['spec']['catalogs'])
    selected = select(record)
    runtime = FactoryRuntime(protocol['spec']['native_library'], catalog_file)
    report = dict(schema='native-full-factory-ownership-profile-v1', status='running',
        reference_protocol_sha256=sha(args.screen / 'protocol.json'), checkpoint_sha256=sha(checkpoint),
        runtime=runtime.metadata, native_library_sha256=sha(protocol['spec']['native_library']),
        sources={str(p.relative_to(ROOT)): sha(p) for p in Path(__file__).parent.glob('*.py')},
        repetitions=args.repeats, warmups=1, cases=[])
    write(args.output / 'report.json', report)
    try:
        for label, sample in selected:
            output = args.output / label
            output.mkdir()
            expected = load_experience(sample)
            trials, generations = [], []
            for index in range(args.repeats + 1):
                guard()
                row, payload, timing = collect(runtime, protocol, sample, actor, critic, output / f'collection-{index}', guard)
                parity = verify(sample, expected, row, payload)
                if index:
                    trials.append(timing)
                if index != 1:
                    shutil.rmtree(output / f'collection-{index}')
            for index in range(args.repeats + 1):
                guard()
                _, generation = generate(runtime, protocol['spec'], sample['identity']['generation_seed'], output / f'generation-{index}', guard)
                require(generation['status'] == 'accepted' and all(generation[k] == sample['generation'][k]
                        for k in ('request_sha256', 'initial_sha256', 'geometry_sha256')), f'generation replay differs: {label}')
                if index:
                    generations.append({k: generation[k] for k in ('seconds', 'admission_seconds', 'setup_seconds')})
                if index != 1:
                    shutil.rmtree(output / f'generation-{index}')
            timings = Timings()
            profile = cProfile.Profile()
            episodes = []
            opening = runtime.open_episode
            def opened(*a, **kw):
                episode = opening(*a, **kw)
                episodes.append(episode)
                return episode
            with ExitStack() as stack:
                stack.enter_context(patch.object(runtime, 'open_episode', opened))
                for name, model in [('actor', actor), ('critic', critic)]:
                    stack.enter_context(patch.object(model, 'forward', timings.wrap(name, model.forward)))
                with profile:
                    row, payload, _ = collect(runtime, protocol, sample, actor, critic, output / 'collection-profiled', guard)
            verify(sample, expected, row, payload)
            profile.dump_stats(str(output / 'collection.pstats'))
            result = dict(label=label, identity=sample['identity'], facilities=sample['row']['instances'],
                initial_score=sample['row']['initial_metrics']['score'], parity=parity,
                collection_trials=trials, generation_trials=generations,
                collection_median_seconds=statistics.median(t['total_seconds'] for t in trials),
                generation_median_seconds=statistics.median(t['seconds'] for t in generations),
                collection_profile=dict(**timings.report(), python=python_summary(profile),
                                        native=[json.loads(e.performance_json()) for e in episodes]))
            report['cases'].append(result)
            write(args.output / 'report.json', report)
            print(json.dumps(dict(label=label, collection=result['collection_median_seconds'],
                                  generation=result['generation_median_seconds'], parity=parity)), flush=True)
        report.update(status='complete', seconds=time.monotonic() - began)
    except BaseException as error:
        report.update(status='failed', error=str(error))
        raise
    finally:
        runtime.close()
        write(args.output / 'report.json', report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--screen', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--seconds', type=float, default=1200)
    args = parser.parse_args()
    if not 1 <= args.repeats <= 11 or not 0 < args.seconds <= 1800:
        parser.error('repeats must be 1..11; seconds must be (0,1800]')
    run(args)
