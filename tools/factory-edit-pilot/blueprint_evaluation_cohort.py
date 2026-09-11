"""Authenticate and load the unchanged evaluation factories from the historical run."""
from copy import copy
from pathlib import Path

from blueprint_ppo_store import read, sha
from blueprint_ppo_update import require
from blueprint_ppo_update import digest
from blueprint_ppo_evaluate import evaluate_case


def load_cohort(path, expected_count, catalogs):
    path = Path(path).resolve()
    value = read(path)
    require(value['schema'] == 'blueprint-evaluation-cohort-v1', f'cohort schema: {path}')
    source = value['source_protocol']
    source_path = path.parent / source['path']
    require(sha(source_path) == source['sha256'], f'historical protocol identity: {source_path}')
    protocol = read(source_path)
    original = protocol['spec']['frozen_starts']
    cases = value['cases']
    require(len(cases) == expected_count == len(original['evaluation_seeds']), 'complete historical evaluation cohort')
    require([c['generation_seed'] for c in cases] == original['evaluation_seeds'], 'historical cohort order and identities')
    sources = {str(path): sha(path), str(source_path): source['sha256']}
    for key, ref in value['catalogs'].items():
        require(sha(Path(catalogs[key])) == ref['sha256'], f'cohort catalog changed: {key}')
    for index, case in enumerate(cases):
        prior = original['entries'][str(case['generation_seed'])]
        require(prior['status'] == 'accepted' and case['initial_metrics'] == prior['generation']['initial_metrics'],
                'historical admitted initial metrics')
        expected_seed = protocol['config']['evaluation']['action_seed_base'] + protocol['config']['seeds'][0] * 100000 + index
        require(case['action_seed'] == expected_seed, 'historical evaluation sampling seed')
        require(set(case['files']) == set(prior['files']) == {'request', 'initial', 'original'}, 'complete cohort input files')
        for key, ref in case['files'].items():
            file = (path.parent / ref['path']).resolve()
            require(file.is_relative_to(path.parent) and sha(file) == ref['sha256'] == prior['files'][key]['sha256'],
                    f'unchanged cohort input: {file}')
            sources[str(file)] = ref['sha256']
        case['directory'] = str(path.parent)
    return cases, sources


def case_runtime(runtime, case):
    result = copy(runtime)
    root = Path(case['directory'])
    result.original = read(root / case['files']['initial']['path'])
    result.request = read(root / case['files']['request']['path'])
    return result


def evaluate_cohort(runtime, model, seed, update, output, guard, cases):
    before = digest(model.state_dict())
    rows = []
    for case in cases:
        selected = case_runtime(runtime, case)
        start = dict(name=f'factory-{case["generation_seed"]}', prefix=[])
        row = evaluate_case(selected, model, seed, update, start, 0, runtime.config['evaluation'],
                            output, guard, case['action_seed'], case['initial_metrics']['score'])
        row.update(generation_seed=case['generation_seed'], action_seed=case['action_seed'],
                   request_sha256=case['files']['request']['sha256'], initial_sha256=case['files']['initial']['sha256'])
        rows.append(row)
    require(digest(model.state_dict()) == before, 'cohort evaluation changed model parameters')
    return rows
