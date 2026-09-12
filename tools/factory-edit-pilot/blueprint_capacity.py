"""Prepare and supervise a source-bound width/depth blueprint training matrix."""
import argparse
from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path

import torch

from blueprint_duration import prepare as prepare_run
from blueprint_edit_input import BlueprintActorCritic
from blueprint_ppo import validate
from blueprint_ppo_store import ROOT, checked, read, reference, sha, write
from blueprint_ppo_update import require
from blueprint_training_matrix import run_matrix


def child_config(base, spec, model, seed, matrix_hash):
    config = deepcopy(base)
    config['purpose'] = f'blueprint-capacity--{model["name"]}'
    config['seeds'] = [seed]
    config['training']['updates'] = spec['updates']
    config['ppo'].update(width=model['width'], message_rounds=model['message_rounds'])
    config['duration'].update(seconds=spec['duration_seconds'], finish_grace_seconds=spec['finish_grace_seconds'],
                              min_free_bytes=spec['min_free_bytes'])
    config['limits'].update(run_seconds=spec['duration_seconds'] + spec['finish_grace_seconds'],
                            max_output_bytes=spec['max_run_bytes'])
    network = BlueprintActorCritic(model['width'], model['message_rounds'])
    config['experiment'] = dict(schema='blueprint-capacity-arm-v1', group=spec['name'],
        matrix_sha256=matrix_hash, variant=model['name'], width=model['width'],
        message_rounds=model['message_rounds'], parameters=sum(p.numel() for p in network.parameters()))
    validate(config)
    return config


def prepare_matrix(spec_path, library, output, cohort, env_file):
    require(not output.exists(), f'fresh matrix output required: {output}')
    spec = read(spec_path)
    require(spec['schema'] == 'blueprint-capacity-matrix-v1', f'matrix schema: {spec_path}')
    require(spec['seeds'] == [17, 29] and len(spec['models']) == 4, 'registered two-seed four-model matrix')
    require({(m['width'], m['message_rounds']) for m in spec['models']} == {(40, 2), (80, 2), (40, 4), (80, 4)},
            'registered width/depth factors')
    require(len({m['name'] for m in spec['models']}) == 4, 'unique architecture identities')
    require(spec['updates'] % 4 == 0 and 0 < spec['last_window_batches'] <= spec['updates'], 'aligned matrix update window')
    base_path = ROOT / spec['base_config']
    base, matrix_hash = read(base_path), sha(spec_path)
    output.mkdir(parents=True)
    arms = []
    for seed_index, seed in enumerate(spec['seeds']):
        models = spec['models'] if seed_index % 2 == 0 else list(reversed(spec['models']))
        for model in models:
            name = f'{model["name"]}-seed-{seed}'
            path = output / 'configs' / f'{name}.json'
            write(path, child_config(base, spec, model, seed, matrix_hash))
            directory = output / name
            prepare_run(path, library, directory, env_file=env_file, evaluation_cohort=cohort)
            arms.append(dict(name=name, seed=seed, model=model, directory=name,
                             protocol=reference(directory / 'protocol.json', output)))
    protocol = dict(schema='blueprint-capacity-protocol-v1', spec=spec, matrix_sha256=matrix_hash,
        source_root=str(ROOT), arms=arms, prepared_at=datetime.now(timezone.utc).isoformat(),
        sources={str(p.relative_to(ROOT)): sha(p) for p in (spec_path, base_path, Path(__file__).resolve(), Path(__file__).with_name('blueprint_training_matrix.py'))})
    write(output / 'matrix.json', protocol)
    write(output / 'state.json', dict(schema='blueprint-capacity-state-v1', status='prepared',
        matrix_sha256=sha(output / 'matrix.json'), arms={a['name']: dict(status='queued') for a in arms}))
    print(dict(status='prepared', arms=len(arms), matrix=str(output)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    prep = sub.add_parser('prepare')
    prep.add_argument('--spec', type=Path, required=True)
    prep.add_argument('--native-library', type=Path, required=True)
    prep.add_argument('--evaluation-cohort', type=Path, required=True)
    execute = sub.add_parser('run')
    execute.add_argument('--cpu-slots', type=int, nargs='+', required=True)
    for command in (prep, execute):
        command.add_argument('--output', type=Path, required=True)
        command.add_argument('--env-file', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    if args.command == 'prepare':
        prepare_matrix(args.spec.resolve(), args.native_library.resolve(), args.output.resolve(),
                       args.evaluation_cohort.resolve(), args.env_file.resolve())
    else:
        run_matrix(args.output.resolve(), args.env_file.resolve(), args.cpu_slots)


if __name__ == '__main__':
    main()
