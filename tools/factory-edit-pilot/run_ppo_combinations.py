"""Prepare and execute the registered eight-cell actor/critic screen sequentially."""
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


def configurations(base, readiness):
    cells = []
    for shared, history, tail in product((False, True), repeat=3):
        if readiness and not tail:
            continue
        config = deepcopy(base)
        factors = treatment(shared, history, tail)
        config['model']['actor_critic'] = factors
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
        cells.append(dict(name=label(factors), factors=factors, config=config))
    random.Random(20260911).shuffle(cells)
    return cells


def prepare_matrix(config_path, output, readiness):
    require(not output.exists(), 'matrix-output', 'matrix requires a fresh output directory', 'prepare', output)
    cells = configurations(read(config_path), readiness)
    output.mkdir(parents=True)
    matrix = dict(schema=SCHEMA, status='preparing', readiness=readiness,
                  source_config=dict(path=str(config_path), sha256=sha(config_path)), cells=[])
    write(output/'matrix.json', matrix)
    for cell in cells:
        config_file = output/(cell['name'] + '.json')
        write(config_file, cell['config'])
        root = output/cell['name']
        protocol = prepare(config_file, root)
        matrix['cells'].append(dict(name=cell['name'], factors=cell['factors'], root=str(root),
            status='prepared', config_sha256=sha(config_file), protocol_sha256=sha(root/'protocol.json'),
            jobs=len(protocol['jobs'])))
        write(output/'matrix.json', matrix)
    matrix['status'] = 'prepared'
    write(output/'matrix.json', matrix)
    print(dict(status='prepared', matrix=str(output/'matrix.json'), cells=len(cells)), flush=True)


def run_matrix(path, env_file):
    matrix = read(path)
    require(matrix['schema'] == SCHEMA and matrix['status'] in ('prepared', 'running'),
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
    run_parser = commands.add_parser('run')
    run_parser.add_argument('--matrix', type=Path, required=True)
    args = parser.parse_args()
    load_environment(args.env_file)
    if args.command == 'prepare':
        prepare_matrix(args.config.resolve(), args.output.resolve(), args.readiness)
    else:
        run_matrix(args.matrix.resolve(), args.env_file.resolve())
