"""Confirm three retained policies with fixed-budget restarts on new geometry."""

import argparse
import json
from pathlib import Path
import subprocess
import time

import torch

from evidence import provenance, write
from geometry_policy_evaluation import admitted_cases
from geometry_restart_diagnosis import evaluate_cell
from mastery_policy_selection import digest, read, select_source
from native import Runtime
from native_training_comparison import policy_hash
from policy import Policy


def validate_protocol(spec):
    total, horizon = spec['total_commands'], spec['attempt_horizon']
    seeds = spec['training_seeds']
    if (spec.get('schema_version') != 1 or spec.get('selection') != 'first_joint_root_mastery'
            or not seeds or any(type(seed) is not int or seed < 0 for seed in seeds)
            or len(seeds) != len(set(seeds)) or set(spec['sources']) != {str(seed) for seed in seeds}
            or type(total) is not int or total <= 0 or type(horizon) is not int or horizon <= 0 or total % horizon):
        raise ValueError('geometry_confirmation.protocol: invalid selection, sources or restart budget')
    banks = []
    for stage in ('development', 'heldout'):
        cell = spec[stage]
        if (type(cell['episodes']) is not int or cell['episodes'] < 2
                or type(cell['required_successes']) is not int
                or not 1 <= cell['required_successes'] <= cell['episodes']
                or len(cell['seed_banks']) != (1 if stage == 'development' else 2)
                or any(type(seed) is not int or seed < 0 for seed in cell['seed_banks'])):
            raise ValueError(f'geometry_confirmation.protocol: invalid {stage} sampling budget')
        banks.extend((seed, cell['episodes']) for seed in cell['seed_banks'])
    if any(max(a, b) < min(a+n, b+m) for i, (a, n) in enumerate(banks) for b, m in banks[i+1:]):
        raise ValueError('geometry_confirmation.protocol: overlapping sampling banks')
    return banks


def verify_training_inputs(root, metadata, train_cases):
    root = root.resolve()
    expected = {digest(path) for path in train_cases.values()}
    actual = set()
    for key, value in metadata['protocol']['cases'].items():
        path = (root/value).resolve()
        recorded = metadata['provenance'][key]['files_sha256'][str(path.relative_to(root))]
        if digest(path) != recorded:
            raise ValueError(f'geometry_confirmation.training_case: {path}: input differs from audited training')
        actual.add(recorded)
    if actual != expected or len(metadata['protocol']['cases']) != len(train_cases):
        raise ValueError('geometry_confirmation.training_split: audited training geometries differ from frozen split')


def learning_contract(metadata):
    return {'config': {k: v for k, v in metadata['config'].items() if k != 'seed'},
            'training_config': {k: v for k, v in metadata['training_config'].items() if k != 'seed'},
            'protocol': {k: v for k, v in metadata['protocol'].items() if k not in ('id', 'training_seeds')}}


def run(root, protocol_path, library, output):
    spec = read(protocol_path)
    banks = validate_protocol(spec)
    cases = admitted_cases(root, spec)
    if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip():
        raise ValueError('geometry_confirmation.source: commit source before confirmation')
    if output.exists():
        raise ValueError(f'geometry_confirmation.output: {output}: choose a fresh directory')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol_path),
              'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
              'selection': {}, 'stages': {}}
    started = time.perf_counter()
    stage, retained = 'selection', {}
    try:
        baseline_contract = None
        for seed in spec['training_seeds']:
            origin = spec['sources'][str(seed)]
            source = root/origin['run']
            selection = {**spec, 'source_commit': origin['source_commit'], 'training_seeds': [seed]}
            metadata, count, saved = select_source(source, selection)
            verify_training_inputs(root, metadata, cases['train'])
            contract = learning_contract(metadata)
            if baseline_contract is not None and contract != baseline_contract:
                raise ValueError(f'geometry_confirmation.learning_contract: seed {seed}: training conditions differ')
            baseline_contract = contract
            source_banks = [(metadata['config']['evaluation_seed_start'], metadata['config']['evaluation_seeds']),
                            (metadata['protocol']['assessment']['seed_start'], metadata['protocol']['assessment']['episodes'])]
            if any(max(a, b) < min(a+n, b+m) for a, n in banks for b, m in source_banks):
                raise ValueError('geometry_confirmation.protocol: outcomes overlap training sampling banks')
            result['selection'][str(seed)] = {'commands': count, 'eligible': count is not None,
                'source_audit_sha256': digest(source/'study-audit.json'),
                'metadata_sha256': digest(source/'metadata.json'), 'historical_training_seconds': metadata['training_seconds']}
            retained[seed] = (metadata, count, saved, source)
        if any(not row['eligible'] for row in result['selection'].values()):
            result.update(status='not_met', diagnostic={'code': 'geometry_confirmation.no_mastered_policy',
                          'stage': stage, 'severity': 'info', 'message': 'At least one training seed lacks joint root mastery'})
            result['total_seconds'] = time.perf_counter()-started
            write(output/'confirmation.json', result)
            return result
        actors, configs, hashes = {}, {}, {}
        for seed, (metadata, count, saved, source) in retained.items():
            config = {**metadata['config'], 'device': 'cpu', 'horizon': spec['attempt_horizon']}
            torch.set_num_threads(config['threads'])
            runtime = Runtime(root, next(iter(cases['train'].values())), config, library)
            current = provenance(runtime, protocol_path)
            original = next(iter(metadata['provenance'].values()))
            required = [root/'tools/factory-edit-pilot/policy.py', *runtime.paths.values()]
            if (not current['git_worktree_clean']
                    or current['native']['build']['files_sha256'] != original['native']['build']['files_sha256']
                    or any(digest(path) != original['files_sha256'][str(path.relative_to(root))] for path in required)):
                raise ValueError(f'geometry_confirmation.source: seed {seed}: changed policy/catalog/native source')
            example, _ = runtime.new_engine().observe()
            actor = Policy(example, normalization=config.get('normalization', 'none'),
                           width_multiplier=config.get('width_multiplier', 1))
            actor.load_state_dict(saved['evaluation_policy'])
            actor.eval()
            actors[seed], configs[seed], hashes[seed] = actor, config, policy_hash(actor.state_dict())
            row = result['selection'][str(seed)]
            row.update(policy_sha256=hashes[seed], checkpoint_sha256=digest(source/f'checkpoint-{count}.pt'),
                       provenance=current)
            torch.save({'schema_version': 1, 'kind': 'inference_policy', 'policy': actor.state_dict(),
                        'config': config, 'dimensions': actor.dimensions, 'source': row}, output/f'selected-policy-{seed}.pt')
            row['export_sha256'] = digest(output/f'selected-policy-{seed}.pt')
        # All source policies are selected and frozen before either evaluation stage.
        write(output/'confirmation.json', result)
        for stage in ('development', 'heldout'):
            cells = []
            result['stages'][stage] = cells
            cell_spec = {**spec[stage], 'total_commands': spec['total_commands']}
            for seed in spec['training_seeds']:
                for bank in spec[stage]['seed_banks']:
                    for key, path in cases[stage].items():
                        runtime = Runtime(root, path, configs[seed], library)
                        cell = evaluate_cell(root, runtime, actors[seed], configs[seed], cell_spec,
                            spec['attempt_horizon'], bank, output/stage/f'seed{seed}'/f'bank{bank}'/key)
                        cells.append({'training_seed': seed, 'case': key, 'seed_start': bank,
                                      'case_training_commands': 0, **cell})
                        if policy_hash(actors[seed].state_dict()) != hashes[seed]:
                            raise ValueError(f'geometry_confirmation.mutation: seed {seed}: policy or BN buffers changed')
                        write(output/'confirmation.json', result)
                        print(json.dumps({'stage': stage, 'training_seed': seed, 'case': key, 'bank': bank,
                                          'successes': cell['translated_successes']}), flush=True)
            if not all(cell['passed'] for cell in cells):
                result.update(status='not_met', diagnostic={'code': f'geometry_confirmation.{stage}_failed',
                              'stage': stage, 'severity': 'info', 'message': f'{stage} reliability criterion not met'})
                break
        else:
            result['status'] = 'passed'
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'geometry_confirmation.interrupted',
                      'stage': stage, 'severity': 'error', 'message': str(error)})
        write(output/'confirmation.json', result)
        raise
    result['total_seconds'] = time.perf_counter()-started
    write(output/'confirmation.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_interop_threads(1)
    result = run(Path(__file__).resolve().parents[2], args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
    print(json.dumps({'status': result['status'], 'output': str(args.output)}))


if __name__ == '__main__':
    main()
