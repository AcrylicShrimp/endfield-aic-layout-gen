"""Prepare and run the registered fresh-input advantage-normalization comparison."""
import argparse
from copy import deepcopy
from datetime import datetime, timezone
from pathlib import Path

import torch

from blueprint_duration import prepare as prepare_run
from blueprint_ppo import validate
from blueprint_ppo_store import ROOT, read, reference, sha, write
from blueprint_ppo_update import require
from blueprint_training_matrix import run_matrix


def child_config(base, spec, variant, seed, matrix_hash, input_directory):
    config = deepcopy(base)
    config['purpose'] = f'blueprint-advantage--{variant}'
    config['seeds'] = [seed]
    config['training']['updates'] = spec['updates']
    config['ppo'].update(width=40, message_rounds=2, advantage_normalization=variant)
    config['training_input'].update(directory=str(input_directory), seed_base=spec['generation_seed_base'])
    config['duration'].update(seconds=spec['duration_seconds'], finish_grace_seconds=spec['finish_grace_seconds'],
                              min_free_bytes=spec['min_free_bytes'])
    config['limits'].update(run_seconds=spec['duration_seconds'] + spec['finish_grace_seconds'],
                            max_output_bytes=spec['max_run_bytes'])
    config['experiment'] = dict(schema='blueprint-advantage-arm-v1', group=spec['name'],
        matrix_sha256=matrix_hash, variant=variant, input_pairing='same actual fresh episode slot across all arms')
    validate(config)
    return config


def prepare_matrix(spec_path, library, output, cohort, env_file):
    require(not output.exists(), f'fresh matrix output required: {output}')
    spec = read(spec_path)
    require(spec['schema'] == 'blueprint-advantage-matrix-v1', f'advantage matrix schema: {spec_path}')
    require(spec['seeds'] == [17, 29, 41] and spec['variants'] == ['standardize', 'center'],
            'registered three-seed two-normalization comparison')
    require(spec['updates'] % 4 == 0 and 0 < spec['last_window_batches'] <= spec['updates'],
            'aligned advantage comparison budget/window')
    base_path = ROOT / spec['base_config']
    base, matrix_hash = read(base_path), sha(spec_path)
    require(base['training_input']['kind'] == 'fresh-random', 'advantage comparison requires restored fresh generation')
    output.mkdir(parents=True)
    arms = []
    for index, seed in enumerate(spec['seeds']):
        variants = spec['variants'] if index % 2 == 0 else list(reversed(spec['variants']))
        for variant in variants:
            name = f'{variant}-seed-{seed}'
            path = output / 'configs' / f'{name}.json'
            write(path, child_config(base, spec, variant, seed, matrix_hash, output / 'training-inputs'))
            directory = output / name
            prepare_run(path, library, directory, env_file=env_file, evaluation_cohort=cohort)
            arms.append(dict(name=name, seed=seed, variant=variant, directory=name,
                             protocol=reference(directory / 'protocol.json', output)))
    source_paths = (spec_path, base_path, Path(__file__).resolve(), Path(__file__).with_name('blueprint_training_matrix.py'))
    protocol = dict(schema='blueprint-advantage-protocol-v1', spec=spec, matrix_sha256=matrix_hash,
        source_root=str(ROOT), arms=arms, prepared_at=datetime.now(timezone.utc).isoformat(),
        sources={str(p.relative_to(ROOT)): sha(p) for p in source_paths})
    write(output / 'matrix.json', protocol)
    write(output / 'state.json', dict(schema='blueprint-advantage-state-v1', status='prepared',
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
