"""Wait for an identified Linux training process, then run audited confirmation once."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time


def process_identity(pid):
    """Bind a wait to process start time, so PID reuse cannot prolong it."""
    directory = Path('/proc')/str(pid)
    try:
        fields = (directory/'stat').read_text().rsplit(')', 1)[1].split()
        command = (directory/'cmdline').read_bytes()
    except FileNotFoundError:
        return None
    if fields[0] == 'Z':
        return None
    return {'start_ticks': fields[19], 'command_sha256': hashlib.sha256(command).hexdigest(),
            'arguments': [part.decode() for part in command.split(b'\0') if part]}


def write(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2)+'\n')
    temporary.replace(path)


def require_complete(source):
    matrix = json.loads((source/'matrix.json').read_text())
    expected = matrix['protocol']['run_order']
    if (matrix['status'] != 'complete' or len(matrix['runs']) != len(expected)
            or any((row['width'], row['seed']) != (wanted['width'], wanted['seed'])
                   or row['status'] != 'complete_and_audited'
                   for row, wanted in zip(matrix['runs'], expected))):
        raise ValueError(f'entity_queue.matrix: {source}: training ended without the complete audited matrix')
    # The confirmation CLI independently validates all protocols, hashes and audits.
    return matrix


def run(args):
    root = Path(__file__).resolve().parents[2]
    source, output = args.source.resolve(), args.output.resolve()
    if sys.platform != 'linux' or args.matrix_pid < 1 or min(args.poll_seconds, args.max_wait_seconds,
                                                           args.confirmation_timeout_seconds) < 1:
        raise ValueError('entity_queue.arguments: require Linux, positive process ID and finite bounds')
    if output.exists():
        raise ValueError(f'entity_queue.output: {output}: choose a fresh directory')
    identity = process_identity(args.matrix_pid)
    if identity is None:
        raise ValueError('entity_queue.process: matrix process is not live at queue admission')
    arguments = identity['arguments']
    if not any(Path(part).name == 'entity_learning_matrix.py' for part in arguments):
        raise ValueError('entity_queue.process: process is not the matrix launcher')
    matrix = json.loads((source/'matrix.json').read_text())
    process_cwd = (Path('/proc')/str(args.matrix_pid)/'cwd').resolve()
    try:
        actual_output = (process_cwd/arguments[arguments.index('--output')+1]).resolve()
    except (ValueError, IndexError) as error:
        raise ValueError('entity_queue.process: matrix command has no output argument') from error
    if actual_output != source or matrix['status'] != 'running':
        raise ValueError('entity_queue.process: source differs from the live running matrix')
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('entity_queue.source: confirmation checkout must be clean')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    output.mkdir(parents=True)
    state = {'schema_version': 1, 'status': 'waiting', 'matrix_pid': args.matrix_pid,
             'process_identity': identity, 'source': str(source), 'confirmation_source_commit': head,
             'training_source_commit': matrix['source_commit'], 'max_wait_seconds': args.max_wait_seconds,
             'confirmation_timeout_seconds': args.confirmation_timeout_seconds}
    state_path = output/'queue.json'
    started = time.monotonic()
    write(state_path, state)
    print(json.dumps({'status': 'waiting', 'matrix_pid': args.matrix_pid, 'output': str(output)}), flush=True)
    try:
        while process_identity(args.matrix_pid) == identity:
            state['wait_seconds'] = time.monotonic()-started
            if state['wait_seconds'] >= args.max_wait_seconds:
                raise TimeoutError('entity_queue.wait_timeout: live matrix exceeded declared queue wait')
            write(state_path, state)
            time.sleep(min(args.poll_seconds, args.max_wait_seconds-state['wait_seconds']))
        require_complete(source)
        if subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() != head:
            raise ValueError('entity_queue.source: confirmation checkout changed while waiting')
        state.update(status='confirming', wait_seconds=time.monotonic()-started)
        write(state_path, state)
        command = [sys.executable, str(root/'tools/factory-edit-pilot/entity_capacity_confirmation.py'),
                   '--source', str(source), '--native-library', str(args.native_library.resolve()),
                   '--output', str(output/'confirmation')]
        with (output/'confirmation.log').open('x') as log:
            subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True,
                           timeout=args.confirmation_timeout_seconds)
        confirmation = json.loads((output/'confirmation/confirmation.json').read_text())
        if confirmation['status'] not in ('passed', 'not_met'):
            raise ValueError('entity_queue.confirmation: child did not complete its declared gates')
        state.update(status='complete', confirmation_status=confirmation['status'])
    except BaseException as error:
        state.update(status='interrupted', diagnostic={'code': 'entity_queue.interrupted', 'severity': 'error',
            'stage': 'confirmation_queue', 'message': str(error), 'entity': str(source)})
        write(state_path, state)
        raise
    state['total_seconds'] = time.monotonic()-started
    write(state_path, state)
    print(json.dumps({'status': state['status'], 'confirmation_status': state['confirmation_status']}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--matrix-pid', type=int, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--poll-seconds', type=int, default=15)
    parser.add_argument('--max-wait-seconds', type=int, default=28800)
    parser.add_argument('--confirmation-timeout-seconds', type=int, default=3600)
    run(parser.parse_args())
