"""Run the declared matched-capacity learning streams on one frozen source."""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import time

from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest


def validate_studies(spec, studies, configs):
    if spec.get('schema_version') != 1 or set(studies) != {'1', '2'} or set(configs) != set(studies):
        raise ValueError('entity_matrix.protocol: widths one and two are required')
    baseline = None
    expected = set()
    for width, study in studies.items():
        config = configs[width]
        if (config['width_multiplier'] != int(width) or study['training_seeds'] != [17, 29, 43]
                or set(study['conditions']) != {spec['condition']}):
            raise ValueError('entity_matrix.protocol: unmatched width, seeds or condition')
        contract = {'study': {k: v for k, v in study.items() if k not in ('id', 'base_config')},
                    'config': {k: v for k, v in config.items() if k != 'width_multiplier'}}
        if baseline is not None and contract != baseline:
            raise ValueError('entity_matrix.learning_contract: study conditions differ beyond width')
        baseline = contract
        expected.update((int(width), seed) for seed in study['training_seeds'])
    order = [(row['width'], row['seed']) for row in spec['run_order']]
    if len(order) != len(expected) or set(order) != expected:
        raise ValueError('entity_matrix.run_order: require every width/seed exactly once')


def run(root, protocol_path, library, output):
    spec = json.loads(protocol_path.read_text())
    study_paths = {key: root/value for key, value in spec['studies'].items()}
    studies = {key: json.loads(path.read_text()) for key, path in study_paths.items()}
    configs = {key: json.loads((root/study['base_config']).read_text()) for key, study in studies.items()}
    validate_studies(spec, studies, configs)
    cases = admitted_cases(root, spec)
    expected = set(cases['train'].values())
    if any(set((root/path).resolve() for path in study['cases'].values()) != expected for study in studies.values()):
        raise ValueError('entity_matrix.training_cases: study differs from frozen training split')
    if output.exists() or subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('entity_matrix.source: require clean committed source and fresh output')
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
    output.mkdir(parents=True)
    state = {'schema_version': 1, 'status': 'running', 'source_commit': source,
             'protocol': spec, 'protocol_sha256': digest(protocol_path), 'runs': []}
    directory = root/'tools/factory-edit-pilot'
    started = time.perf_counter()
    try:
        for job in spec['run_order']:
            width, seed = job['width'], job['seed']
            protocol = study_paths[str(width)]
            name = f'width{width}-seed{seed}'
            path = output/name
            row = {**job, 'directory': str(path), 'protocol_sha256': digest(protocol), 'status': 'running'}
            state['runs'].append(row)
            write(output/'matrix.json', state)
            print(json.dumps({'event': 'entity_matrix_started', **row}), flush=True)
            clock = time.perf_counter()
            with (output/f'{name}.log').open('w') as log:
                subprocess.run([sys.executable, str(directory/'case_curriculum_study.py'), '--root', str(root),
                    '--protocol', str(protocol), '--condition', spec['condition'], '--seed', str(seed),
                    '--native-library', str(library), '--output', str(path)], cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
                row['training_and_evaluation_wall_seconds'] = time.perf_counter()-clock
                clock = time.perf_counter()
                subprocess.run([sys.executable, str(directory/'case_curriculum_audit.py'), '--root', str(root),
                    '--protocol', str(protocol), '--output', str(path), '--source-commit', source],
                    cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)
                row['audit_wall_seconds'] = time.perf_counter()-clock
            row['status'] = 'complete_and_audited'
            row['audit_sha256'] = digest(path/'study-audit.json')
            write(output/'matrix.json', state)
            print(json.dumps({'event': 'entity_matrix_complete', **row}), flush=True)
        state['status'] = 'complete'
    except BaseException as error:
        state.update(status='interrupted', diagnostic={'code': 'entity_matrix.interrupted', 'severity': 'error',
            'stage': 'entity_matrix', 'message': str(error), 'entity': str(output)})
        raise
    finally:
        state['wall_seconds'] = time.perf_counter()-started
        write(output/'matrix.json', state)
    return state


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(Path(__file__).resolve().parents[2], args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())


if __name__ == '__main__':
    main()
