"""Explicit immutable initial factories for controlled executor comparisons."""
from copy import deepcopy
from pathlib import Path
import shutil
import time

from ppo_harness_state import require, read, write, sha


def load(path, config, scope='all'):
    path = Path(path).resolve()
    value = read(path)
    require(value.get('schema') == 'ppo-frozen-starts-v1' and isinstance(value.get('entries'), dict),
            'frozen-start-schema', 'expected ppo-frozen-starts-v1 entries', 'prepare', path)
    require(scope in ('all', 'evaluation'), 'frozen-start-scope', 'expected all or evaluation scope', 'prepare', path)
    schedules = [(config['evaluation'], config['evaluation']['attempts'])]
    if scope == 'all':
        schedules.append((config['training'], config['training']['batches'] * config['training']['attempts']))
    evaluation_seeds = [config['evaluation']['generation_seed_base'] + seed * 100000 + i
                        for seed in config['seeds'] for i in range(config['evaluation']['attempts'])]
    needed = {schedule['generation_seed_base'] + seed * 100000 + i
              for seed in config['seeds']
              for schedule, count in schedules
              for i in range(count)}
    files, entries = {path}, {}
    for seed in sorted(needed):
        entry = value['entries'].get(str(seed))
        require(entry is not None and entry.get('generation_seed') == seed
                and entry.get('status') in ('accepted', 'rejected'),
                'frozen-start-missing', 'scheduled generation seed absent or invalid in input manifest', 'prepare', seed)
        require(entry['generation']['generation_seed'] == seed and entry['generation']['status'] == entry['status'],
                'frozen-start-identity', 'entry and source generation identity disagree', 'prepare', seed)
        require(set(entry['files']) == ({'request', 'initial', 'original'} if entry['status'] == 'accepted' else set()),
                'frozen-start-files', 'accepted starts require request, initial and original files', 'prepare', seed)
        entry = deepcopy(entry)
        for key, reference in entry['files'].items():
            source = Path(reference['path'])
            require(source.is_absolute() and source.is_file() and sha(source) == reference['sha256'],
                    'frozen-start-hash', 'frozen input file is absent or its bytes changed', 'prepare', source)
            expected = entry['generation'].get(key + '_sha256')
            require(expected is None or expected == reference['sha256'], 'frozen-start-baseline-hash',
                    'frozen input differs from the retained baseline identity', 'prepare', source)
            files.add(source)
        entries[str(seed)] = entry
    return dict(manifest=str(path), manifest_sha256=sha(path), entries=entries,
                scope=scope, evaluation_seeds=evaluation_seeds), files


def applies(frozen, seed):
    # Route by the declared schedule, never by whether an entry happens to exist.
    return frozen.get('scope', 'all') == 'all' or seed in frozen['evaluation_seeds']


def materialize(runtime, spec, seed, directory, guard):
    began = time.monotonic()
    frozen = spec['frozen_starts']
    entry = frozen['entries'].get(str(seed))
    require(entry is not None, 'frozen-start-missing', 'generation seed is not in the frozen manifest', 'generation', seed)
    guard()
    directory = Path(directory)
    directory.mkdir()
    generation = deepcopy(entry['generation'])
    generation.update(frozen_start=dict(manifest=frozen['manifest'], sha256=frozen['manifest_sha256'],
                                       baseline_result=entry['baseline_result']),
                      source_generation_seconds=generation['seconds'], admission_seconds=0., setup_seconds=0.)
    # Give retention a task-owned location; no baseline artifact is ever removed.
    admission = directory / 'admission'
    admission.mkdir()
    generation['original_attempt']['directory'] = str(admission)
    case = None
    if entry['status'] == 'accepted':
        case = dict(id=f'generated-{seed}', dialect=generation['case']['dialect'],
                    instances=generation['case']['instances'], files_sha256={})
        for key, reference in entry['files'].items():
            source = Path(reference['path'])
            require(sha(source) == reference['sha256'], 'frozen-start-hash', 'frozen input changed after preparation', 'generation', source)
            target = directory / f'{key}.json'
            shutil.copyfile(source, target)
            require(sha(target) == reference['sha256'], 'frozen-start-copy', 'copied input hash differs', 'generation', target)
            case[key] = str(target.resolve())
            case['files_sha256'][target.name] = reference['sha256']
        generation['case'] = case
        admitted = time.monotonic()
        check = runtime.open_episode(case, 1, spec['generation']['setup_seconds'], spec['selection']['route_work'])
        require(check.summary()['initial_metrics'] == generation['initial_metrics'], 'frozen-start-metrics',
                'native admission metrics differ from the retained initial factory', 'generation', seed)
        reference = directory / 'reference.json'
        write(reference, dict(schema_version=2, backend=runtime.metadata['backend'], factory_validated=True,
                              request_sha256=generation['request_sha256'],
                              initial_sha256=generation['initial_sha256'], provenance=runtime.metadata['provenance']))
        case['files_sha256'][reference.name] = sha(reference)
        generation['admission_seconds'] = time.monotonic() - admitted
    generation['seconds'] = time.monotonic() - began
    write(directory / 'generation.json', generation)
    guard()
    return case, generation
