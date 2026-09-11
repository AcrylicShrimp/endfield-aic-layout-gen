"""Prepare and execute registered PPO factor, capacity or learning-rate screens."""
import argparse
from copy import deepcopy
from itertools import product
from pathlib import Path
import random
import subprocess
import sys
import time

from ppo_actor_critic import treatment, label
from ppo_harness_environment import load_environment
from ppo_harness_protocol import prepare, validate
from ppo_harness_state import read, write, sha, require, diagnostic

SCHEMA = 'ppo-actor-critic-matrix-v1'
CAPACITY_SCHEMA = 'ppo-shared-capacity-matrix-v1'
RATE_SCHEMA = 'ppo-learning-rate-matrix-v1'
LOWER_RATES = {'lr-1e-4': 0.0001, 'lr-3e-5': 0.00003}


def configurations(base, readiness, shared_capacity=False, learning_rates=False):
    require(not (shared_capacity and learning_rates), 'matrix-mode', 'select one registered matrix mode', 'prepare')
    if learning_rates:
        from ppo_spatial_model import shared_capacity_specification
        require(base.get('model') == shared_capacity_specification(True, True)
                and base['ppo']['learning_rate'] == 0.0003, 'matrix-model',
                'lower rates require width-40 shared history/critic-tail and the 0.0003 base rate', 'prepare')
    if shared_capacity:
        from ppo_spatial_model import SHARED_CAPACITY_SCHEMA
        require(base.get('model', {}).get('schema') == SHARED_CAPACITY_SCHEMA,
                'matrix-model', 'shared capacity requires its explicit width-40 model', 'prepare')
    if learning_rates:
        treatments = [dict(name=name, factors=treatment(True, True, True), learning_rate=rate)
                      for name, rate in LOWER_RATES.items()]
    else:
        treatments = [dict(name=label(treatment(*bits)), factors=treatment(*bits))
                      for bits in product((True,) if shared_capacity else (False, True), (False, True), (False, True))]
    cells = []
    for cell in treatments:
        if readiness and not cell['factors']['critic_after_kl']:
            continue
        config = deepcopy(base)
        config['model']['actor_critic'] = cell['factors']
        if learning_rates:
            config['ppo']['learning_rate'] = cell['learning_rate']
        if readiness:
            config.update(purpose='readiness', seeds=[17])
            config['training'].update(batches=2, attempts=16)
            config['evaluation'].update(at=[0, 2], attempts=8)
            config['execution']['workers'] = 8
            config['limits']['run_seconds'] = 900
            config['ppo']['target_kl'] = 1e-8
            config['retention']['training_batches'] = [1, 2]
            config['tracking'].update(backend='none', mode='offline')
        validate(config)
        cells.append(dict(**cell, config=config))
    random.Random(20260911).shuffle(cells)
    return cells


def prepare_matrix(config_path, output, readiness, shared_capacity=False, learning_rates=False):
    require(not output.exists(), 'matrix-output', 'matrix requires a fresh output directory', 'prepare', output)
    cells = configurations(read(config_path), readiness, shared_capacity, learning_rates)
    output.mkdir(parents=True)
    matrix = dict(schema=RATE_SCHEMA if learning_rates else CAPACITY_SCHEMA if shared_capacity else SCHEMA,
                  status='preparing', readiness=readiness,
                  source_config=dict(path=str(config_path), sha256=sha(config_path)), cells=[])
    write(output/'matrix.json', matrix)
    for cell in cells:
        config_file = output/(cell['name'] + '.json')
        write(config_file, cell['config'])
        root = output/cell['name']
        protocol = prepare(config_file, root)
        matrix['cells'].append(dict(**{k: v for k, v in cell.items() if k != 'config'}, root=str(root),
            status='prepared', config_sha256=sha(config_file), protocol_sha256=sha(root/'protocol.json'),
            jobs=len(protocol['jobs'])))
        write(output/'matrix.json', matrix)
    matrix['status'] = 'prepared'
    write(output/'matrix.json', matrix)
    print(dict(status='prepared', matrix=str(output/'matrix.json'), cells=len(cells)), flush=True)


def run_matrix(path, env_file):
    matrix = read(path)
    require(matrix['schema'] in (SCHEMA, CAPACITY_SCHEMA, RATE_SCHEMA) and matrix['status'] in ('prepared', 'running'),
            'matrix-state', 'expected prepared/running combination matrix', 'run', path)
    matrix.update(status='running', started_epoch=matrix.get('started_epoch', time.time()))
    write(path, matrix)
    try:
        for cell in matrix['cells']:
            root = Path(cell['root'])
            require(sha(root/'protocol.json') == cell['protocol_sha256'], 'matrix-protocol',
                    'prepared cell protocol changed', 'run', root)
            state = read(root/'state.json')
            if state['status'] == 'complete':
                cell['status'] = 'complete'
                write(path, matrix)
                continue
            require(state['status'] == 'prepared', 'matrix-cell-state',
                    'unfinished cells require explicit inspection before retry', 'run', root)
            cell.update(status='running', started_epoch=time.time())
            write(path, matrix)
            print(dict(cell=cell['name'], status='running'), flush=True)
            command = [sys.executable, str(Path(__file__).with_name('ppo_harness.py')),
                       '--env-file', str(env_file), 'run', '--run', str(root)]
            with (root/'matrix-run.log').open('a') as log:
                result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
            state = read(root/'state.json')
            cell.update(status=state['status'], exit_code=result.returncode, finished_epoch=time.time(),
                        state_sha256=sha(root/'state.json'))
            write(path, matrix)
            require(result.returncode == 0 and state['status'] == 'complete', 'matrix-cell-failed',
                    f'cell ended with status {state["status"]}', 'run', root)
            print(dict(cell=cell['name'], status='complete'), flush=True)
        matrix.update(status='complete', finished_epoch=time.time())
    except BaseException as error:
        matrix.update(status='failed', diagnostic=diagnostic(error, 'matrix', path), finished_epoch=time.time())
        raise
    finally:
        write(path, matrix)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--env-file', type=Path, default=Path(__file__).resolve().parents[2]/'.env')
    commands = parser.add_subparsers(dest='command', required=True)
    prepare_parser = commands.add_parser('prepare')
    prepare_parser.add_argument('--config', type=Path, required=True)
    prepare_parser.add_argument('--output', type=Path, required=True)
    prepare_parser.add_argument('--readiness', action='store_true')
    modes = prepare_parser.add_mutually_exclusive_group()
    modes.add_argument('--shared-capacity', action='store_true',
                                help='Run the four width-40 shared history/schedule combinations')
    modes.add_argument('--learning-rates', action='store_true',
                       help='Run 0.0001 and 0.00003 with the fixed width-40 shared history/critic-tail model')
    run_parser = commands.add_parser('run')
    run_parser.add_argument('--matrix', type=Path, required=True)
    args = parser.parse_args()
    load_environment(args.env_file)
    if args.command == 'prepare':
        prepare_matrix(args.config.resolve(), args.output.resolve(), args.readiness, args.shared_capacity, args.learning_rates)
    else:
        run_matrix(args.matrix.resolve(), args.env_file.resolve())
