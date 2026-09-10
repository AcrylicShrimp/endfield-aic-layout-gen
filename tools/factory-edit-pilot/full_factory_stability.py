"""Freeze and execute fresh four-arm on-policy update-stability comparisons."""
import argparse
from copy import deepcopy
import math
from pathlib import Path
import random
import signal
import time

PROCESS_BEGAN = time.monotonic()

import torch

from full_factory_curriculum import source_manifest
from full_factory_large_learning import read, verify_sources
from full_factory_on_policy import parameter_hash
from full_factory_on_policy_collection import CollectionError, ProcessTreeMemory, collect_group
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy, SCHEMA
from full_factory_screen import sha, write
from full_factory_stability_panel import load_panel, probabilities, summarize
from full_factory_stability_update import CONTRACT, checkpoint_value, require, update_group


ARMS = [dict(name='ordinary', learning_rate=.0003, skip_flat=False),
        dict(name='lower-rate', learning_rate=.00003, skip_flat=False),
        dict(name='skip-flat', learning_rate=.0003, skip_flat=True),
        dict(name='lower-rate-skip-flat', learning_rate=.00003, skip_flat=True)]
COMMON_KEYS = ('horizon', 'group_episodes', 'route_work', 'observation_array_bytes', 'episode_state_bytes',
               'temperature', 'balanced', 'memory', 'entropy_coefficient', 'gradient_clip')


def config_check(config, smoke=False):
    require(config['schema_version'] == 1 and type(config['groups']) is int and config['groups'] == (1 if smoke else 50), 'study version/group count')
    require(config['arms'] == ARMS, 'registered stability arms')
    require(all(type(a['skip_flat']) is bool and type(a['learning_rate']) in (int, float) for a in config['arms']), 'arm field types')
    require(config['schedule_seed'] == config['model_seed'] == 17 and config['training_seed_base'] == 3100000,
            'registered training seeds')
    require(config['evaluation_banks'] == [3200000, 3200001], 'registered evaluation banks')
    for key, limit in dict(episode_seconds=300, group_seconds=1200, arm_seconds=7200, study_seconds=28800,
                           evaluation_seconds=14400, sampled_rss_bytes=12*1024**3, panel_observation_bytes=512*1024**2).items():
        require(type(config[key]) is int and 0 < config[key] <= limit, f'study bound: {key}')


def schedule(cases, config):
    require(len(cases) == 10 and len(set(cases)) == 10, 'training reset membership')
    order = list(cases)
    random.Random(config['schedule_seed']).shuffle(order)
    return [dict(group=g, case=order[g % len(order)], sample_seeds=[config['training_seed_base']+4*g+i for i in range(4)])
            for g in range(config['groups'])]


def collection_mode(directory, rss_limit):
    protocol = read(directory/'protocol.json')
    report = read(directory/'report.json')
    require(report['status'] == 'complete' and report['protocol_sha256'] == sha(directory/'protocol.json'), 'complete collection matrix')
    verify_sources(protocol)
    expected = {(c['id'], repeat, workers) for c in protocol['conditions'] for repeat in range(protocol['study']['repeats'])
                for workers in (1, 2, 4)}
    require(len(expected) == len(report['rows']) and
            {(r['condition'], r['repetition'], r['workers']) for r in report['rows']} == expected, 'collection matrix membership')
    totals = []
    for workers in (1, 2, 4):
        rows = [r for r in report['rows'] if r['workers'] == workers]
        require(all(r['semantic_identity'] is True and r['selected'] == 128 and not r['execution_memory']['errors'] for r in rows),
                'collection semantics or memory measurement')
        require(all(math.isfinite(r['execution_group_seconds']) and r['execution_group_seconds'] > 0 for r in rows), 'collection timing')
        peak = max(r['execution_memory']['peak_bytes'] for r in rows)
        totals.append(dict(workers=workers, execution_seconds=sum(r['execution_group_seconds'] for r in rows),
                           peak_sampled_rss_bytes=peak, eligible=peak <= rss_limit))
    eligible = [row for row in totals if row['eligible']]
    require(bool(eligible), 'no measured collector fits the declared RSS bound')
    selected = min(eligible, key=lambda row: (row['execution_seconds'], row['workers']))
    return dict(workers=selected['workers'], comparison=totals, report_sha256=sha(directory/'report.json'),
                rule='Lowest summed complete-group execution time across all matched conditions; ties prefer fewer workers.')


def prepare(args):
    config = read(args.config)
    config_check(config)
    parent, result = load_result(args.training)
    audit = read(args.training/'training-audit.json')
    require(audit['status'] == 'passed' and audit['report_sha256'] == sha(args.training/'training-result.json'), 'parent training audit')
    evaluation = read(args.evaluation/'protocol.json')
    evaluated = read(args.evaluation/'report.json')
    proof = read(args.evaluation/'audit.json')
    require(evaluated['status'] == 'complete' and evaluated['protocol_sha256'] == sha(args.evaluation/'protocol.json') and
            proof['status'] == 'passed' and proof['report_sha256'] == sha(args.evaluation/'report.json'), 'development evaluation audit')
    require(Path(evaluation['training']).resolve() == args.training.resolve() and len(evaluation['cases']) == 35, 'development case source')
    verify_sources(evaluation)
    require({k: sha(f) for k, f in read(evaluation['catalogs']).items()} == {k: sha(f) for k, f in read(parent['catalogs']).items()},
            'development catalog identity')
    diagnosis = read(args.diagnosis/'protocol.json')
    diagnosed = read(args.diagnosis/'report.json')
    require(diagnosed['status'] == 'complete' and diagnosed['protocol_sha256'] == sha(args.diagnosis/'protocol.json'), 'diagnosis panel source')
    require(diagnosis['sources'][str((args.training/'training-result.json').resolve())] == sha(args.training/'training-result.json'),
            'diagnosis training identity')
    panel = diagnosis['panel']
    require(len(panel) == 10 and {r['case'] for r in panel} == set(parent['training_cases']), 'diagnosis panel membership')
    mode = collection_mode(args.collection, config['sampled_rss_bytes'])
    arm = next(a for a in result['arms'] if a['initialization'] == 'imitation-warm')
    initial = Path(arm['initial'])
    require(sha(initial) == arm['initial_sha256'], 'warm initialization hash')
    initial_value = torch.load(initial, map_location='cpu', weights_only=False)
    require(initial_value['schema'] == SCHEMA and not initial_value['optimizer']['state'], 'warm initialization contract')
    sources = dict(parent['sources'])

    def bind(file, expected=None):
        file = Path(file).resolve()
        identity = sha(file)
        require(expected is None or identity == expected, f'source identity: {file}')
        sources[str(file)] = identity

    bind(args.config)
    bind(initial)
    for directory, names in ((args.training, ('protocol.json', 'training-result.json', 'training-audit.json')),
                             (args.evaluation, ('protocol.json', 'report.json', 'audit.json')),
                             (args.diagnosis, ('protocol.json', 'report.json')),
                             (args.collection, ('protocol.json', 'report.json'))):
        for name in names:
            bind(directory/name)
    for row in panel:
        for name in ('states.json.gz', 'episode.json.gz'):
            file = Path(row['directory'])/name
            bind(file, diagnosis['sources'][str(file.resolve())])
    cases = evaluation['cases']
    require(len({c['id'] for c in cases}) == 35 and set(parent['training_cases']) <= {c['id'] for c in cases}, 'case membership')
    require(all(c['split'] == 'train' for c in cases if c['id'] in parent['training_cases']), 'training case split')
    for case in cases:
        for key, name in (('initial', 'initial.json'), ('request', 'request.json'), ('original', 'original-request.json')):
            bind(case[key], case['files_sha256'][name])
    common = {key: parent['config'][key] for key in COMMON_KEYS}
    require(common['horizon'] == 32 and common['group_episodes'] == 4 and common['entropy_coefficient'] == .001 and
            common['gradient_clip'] == .5 and common['temperature'] == 1 and common['balanced'] and common['memory'], 'parent common settings')
    common['episode_seconds'] = config['episode_seconds']
    if args.smoke:
        config = dict(config, groups=1)
        common['horizon'] = 2
    protocol = dict(schema_version=1, learning_contract=CONTRACT, mode='native-smoke' if args.smoke else 'study',
                    experiment='fresh-on-policy-update-stability', study=config, config=common, collection=mode,
                    config_path=str(args.config.resolve()), parent_training=str(args.training.resolve()),
                    initial_checkpoint=str(initial.resolve()), initial_policy_sha256=initial_value['policy_sha256'],
                    cases=cases, training_cases=parent['training_cases'], panel=panel, schedule=schedule(parent['training_cases'], config),
                    evaluation_cases=[c['id'] for c in cases], evaluation_methods=['initial', *(a['name']+'-terminal' for a in config['arms'])],
                    evaluation_scope='Previously evaluated development factories; new paired sample banks, not untouched transfer cases.',
                    depth=parent['depth'], width=parent['width'], policy_schema=SCHEMA,
                    native_library=parent['native_library'], catalogs=parent['catalogs'], sources=sources, source_sha256=source_manifest())
    verify_sources(protocol)
    args.output.mkdir()
    write(args.output/'protocol.json', protocol)
    print(dict(status='frozen', mode=protocol['mode'], workers=mode['workers'],
               training_groups=config['groups']*4, training_actions=config['groups']*4*4*common['horizon']), flush=True)


def load_checkpoint(protocol, protocol_hash, arm, file):
    value = torch.load(file, map_location='cpu', weights_only=False)
    require(value['learning_contract'] == CONTRACT and value['protocol_sha256'] == protocol_hash and value['arm'] == arm['name'],
            f'checkpoint contract: {file}')
    require(value['schema'] == SCHEMA and (value['depth'], value['width']) == (protocol['depth'], protocol['width']), f'checkpoint schema: {file}')
    require(type(value['groups_collected']) is int and 0 <= value['groups_collected'] <= protocol['study']['groups'], f'checkpoint group count: {file}')
    model = FullFactoryPolicy(protocol['depth'], protocol['width'])
    model.load_state_dict(value['policy'], strict=True)
    optimizer = torch.optim.Adam(model.parameters(), lr=arm['learning_rate'])
    optimizer.load_state_dict(value['optimizer'])
    torch.set_rng_state(value['rng'])
    verified = checkpoint_value(protocol, protocol_hash, arm, model, optimizer, value['groups_collected'], value['optimizer_updates'])
    require(verified['policy_sha256'] == value['policy_sha256'], f'checkpoint policy: {file}')
    return model, optimizer, value


def save_checkpoint(file, value):
    require(not file.exists(), f'checkpoint already exists: {file}')
    temporary = file.with_suffix('.pt.tmp')
    torch.save(value, temporary)
    temporary.replace(file)


def arm_reports(protocol, initials, groups, arm_seconds):
    result = []
    for arm in protocol['study']['arms']:
        name = arm['name']
        own = [g for g in groups if g['arm'] == name]
        initial = initials.get(name)
        terminal = own[-1] if own else initial
        result.append(dict(**arm, groups_collected=len(own), optimizer_updates=sum(g['optimizer_step_applied'] for g in own),
                           skipped_groups=sum(not g['optimizer_step_applied'] for g in own),
                           initial=initial['checkpoint'] if initial else None, initial_sha256=initial['checkpoint_sha256'] if initial else None,
                           terminal=terminal['checkpoint'] if terminal else None, terminal_sha256=terminal['checkpoint_sha256'] if terminal else None,
                           seconds=arm_seconds[name]))
    return result


def train(args):
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    protocol_file = args.screen/'protocol.json'
    protocol = read(protocol_file)
    protocol_hash = sha(protocol_file)
    require(protocol['learning_contract'] == CONTRACT and protocol['mode'] in ('study', 'native-smoke'), 'training protocol contract')
    smoke = protocol['mode'] == 'native-smoke'
    config = protocol['study']
    config_check(config, smoke)
    registered = read(protocol['config_path'])
    if smoke:
        registered = dict(registered, groups=1)
    require(config == registered, 'registered configuration differs from protocol')
    require(protocol['schedule'] == schedule(protocol['training_cases'], config), 'training schedule')
    require(protocol['config']['horizon'] == (2 if smoke else 32) and protocol['config']['group_episodes'] == 4, 'rollout shape')
    require(not any((args.screen/name).exists() for name in ('training-result.json', 'progress.json', 'partial.json', 'initial-panel.json')),
            'existing training output; use a fresh prepared screen')
    verify_sources(protocol)
    initials, groups = {}, []
    arm_seconds = {arm['name']: 0.0 for arm in config['arms']}
    active = dict(stage='initialization', arm=None, group=None)

    def payload(status):
        return dict(schema_version=1, learning_contract=CONTRACT, status=status, mode=protocol['mode'], protocol_sha256=protocol_hash,
                    seconds=time.monotonic()-PROCESS_BEGAN, arms=arm_reports(protocol, initials, groups, arm_seconds),
                    groups=groups, rows=[row for group in groups for row in group['episodes']],
                    requested_groups=config['groups']*len(config['arms']),
                    requested_actions=config['groups']*len(config['arms'])*4*protocol['config']['horizon'],
                    active=active, teacher_labels_used=False, evaluation_performed=False)

    def study_guard(memory):
        if memory.errors:
            raise CollectionError('on-policy.stability-memory-measurement', str(memory.errors))
        if time.monotonic()-PROCESS_BEGAN >= config['study_seconds'] or memory.peak_bytes > config['sampled_rss_bytes']:
            raise CollectionError('on-policy.stability-resource-bound', f'Study time or sampled-RSS bound: {active}')

    try:
        with ProcessTreeMemory() as memory:
            panel = load_panel(protocol, lambda: study_guard(memory))
            warm = torch.load(protocol['initial_checkpoint'], map_location='cpu', weights_only=False)
            initial_probabilities = None
            for arm in config['arms']:
                active = dict(stage='initialization', arm=arm['name'], group=None)
                began = time.monotonic()
                torch.manual_seed(config['model_seed'])
                model = FullFactoryPolicy(protocol['depth'], protocol['width'])
                model.load_state_dict(warm['policy'], strict=True)
                require(parameter_hash(model) == protocol['initial_policy_sha256'], 'common warm policy')
                optimizer = torch.optim.Adam(model.parameters(), lr=arm['learning_rate'])
                value = checkpoint_value(protocol, protocol_hash, arm, model, optimizer, 0, 0)
                directory = args.screen/arm['name']
                directory.mkdir()
                file = directory/'initial.pt'
                save_checkpoint(file, value)
                initials[arm['name']] = dict(arm=arm['name'], checkpoint=str(file.resolve()), checkpoint_sha256=sha(file))
                arm_seconds[arm['name']] += time.monotonic()-began
                study_guard(memory)
                if initial_probabilities is None:
                    initial_probabilities = probabilities(protocol, model, panel, lambda: study_guard(memory))
            write(args.screen/'initial-panel.json', dict(schema_version=1, protocol_sha256=protocol_hash,
                  policy_sha256=protocol['initial_policy_sha256'], action_selection_performed=False,
                  rows=summarize(panel, initial_probabilities, initial_probabilities, initial_probabilities)))
        require(not memory.errors, 'initialization memory measurement')
        study_guard(memory)
        initialization_memory = memory.report()
        terminals = dict(initials)
        cases = {case['id']: case for case in protocol['cases']}
        for item in protocol['schedule']:
            # Complete groups rotate arm order while each policy retains its own state.
            offset = item['group'] % len(config['arms'])
            order = config['arms'][offset:] + config['arms'][:offset]
            for arm in order:
                name = arm['name']
                directory = args.screen/name/f'group-{item["group"]:04d}'
                active = dict(stage='collection', arm=name, group=item['group'], case=item['case'], directory=str(directory.resolve()))
                group_began = time.monotonic()
                try:
                    with ProcessTreeMemory() as memory:
                        def guard():
                            study_guard(memory)
                            elapsed = time.monotonic()-group_began
                            if elapsed >= config['group_seconds'] or arm_seconds[name]+elapsed >= config['arm_seconds']:
                                raise CollectionError('on-policy.stability-resource-bound', f'Group or active-arm time bound: {active}')
                        guard()
                        before_path = Path(terminals[name]['checkpoint'])
                        before_sha = terminals[name]['checkpoint_sha256']
                        require(sha(before_path) == before_sha, f'preceding checkpoint changed: {before_path}')
                        model, optimizer, before = load_checkpoint(protocol, protocol_hash, arm, before_path)
                        require(before['groups_collected'] == item['group'], 'arm collection prefix')
                        previous = probabilities(protocol, model, panel, guard)
                        attempts = [dict(attempt=i, seed=seed) for i, seed in enumerate(item['sample_seeds'])]
                        episode_rows, collection = collect_group(protocol_file, before_path, item['case'], attempts, directory,
                            group=item['group'], workers=protocol['collection']['workers'], group_seconds=config['group_seconds'],
                            policy_sha256=before['policy_sha256'], episode_seconds=config['episode_seconds'],
                            rss_limit_bytes=config['sampled_rss_bytes'], memory=memory, guard=guard)
                        collected = time.monotonic()
                        episode_rows = [dict(row, arm=name, group=item['group'], attempt=i, phase='training',
                                             behavior_checkpoint=str(before_path), behavior_checkpoint_sha256=before_sha)
                                        for i, row in enumerate(episode_rows)]
                        active['stage'] = 'gradient-update'
                        outcome = update_group(protocol, cases[item['case']], arm, model, optimizer, episode_rows, guard)
                        updated = time.monotonic()
                        active['stage'] = 'panel-checkpoint'
                        current = probabilities(protocol, model, panel, guard)
                        panel_rows = summarize(panel, current, initial_probabilities, previous)
                        updates = before['optimizer_updates']+int(outcome['optimizer_step_applied'])
                        value = checkpoint_value(protocol, protocol_hash, arm, model, optimizer, item['group']+1, updates)
                        file = directory/'checkpoint.pt'
                        save_checkpoint(file, value)
                        guard()
                    require(not memory.errors, 'group memory measurement')
                    guard()
                    record = dict(arm=name, group=item['group'], case=item['case'], sample_seeds=item['sample_seeds'],
                                  **outcome, groups_collected=item['group']+1, optimizer_updates=updates,
                                  episodes=episode_rows, panel=panel_rows, memory=memory.report(),
                                  collection_seconds=collected-group_began, gradient_update_seconds=updated-collected,
                                  measured_group_seconds=time.monotonic()-group_began,
                                  before_checkpoint=str(before_path), before_checkpoint_sha256=before_sha,
                                  checkpoint=str(file.resolve()), checkpoint_sha256=sha(file),
                                  collection_sha256=sha(directory/'collection.json'), completion_order=collection['completion_order'])
                    write(directory/'group.json', record)
                    groups.append(record)
                    terminals[name] = record
                    active['stage'] = 'group-committed'
                    write(args.screen/'progress.json', payload('incomplete'))
                    print(dict(arm=name, groups_collected=item['group']+1, optimizer_updates=updates,
                               flat_returns=outcome['flat_returns'], step_applied=outcome['optimizer_step_applied'],
                               mean_panel_tv=sum(r['total_variation_from_previous'] for r in panel_rows)/len(panel_rows),
                               group_seconds=time.monotonic()-group_began), flush=True)
                    guard()
                finally:
                    # Includes result serialization and sampler cleanup in active-arm accounting.
                    arm_seconds[name] += time.monotonic()-group_began
        active = dict(stage='finalization', arm=None, group=None)
        verify_sources(protocol)
        require(time.monotonic()-PROCESS_BEGAN < config['study_seconds'], 'study time exhausted at finalization')
        final = payload('complete')
        final['initialization_memory'] = initialization_memory
        final['scope'] = 'Native mechanical smoke only.' if smoke else 'Fresh equal-experience four-arm training; endpoint evaluation pending.'
        write(args.screen/'training-result.json', final)
    except BaseException as error:
        diagnostic = dict(code='on-policy.stability-training-failed', severity='error', stage=active['stage'],
                          arm=active['arm'], group=active['group'], message=str(error) or type(error).__name__)
        if isinstance(error, CollectionError):
            diagnostic['cause'] = error.diagnostic
        failure = payload('incomplete')
        failure['diagnostic'] = diagnostic
        failure['uncommitted_group_directory'] = active.get('directory') if active['stage'] != 'group-committed' else None
        write(args.screen/'partial.json', failure)
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    freeze = sub.add_parser('prepare')
    for name in ('training', 'evaluation', 'diagnosis', 'collection', 'config', 'output'):
        freeze.add_argument('--'+name, required=True, type=Path)
    freeze.add_argument('--smoke', action='store_true', help='Explicit one-group/two-action native mechanical check; not study evidence')
    sub.add_parser('train').add_argument('--screen', required=True, type=Path)
    args = parser.parse_args()

    def interrupted(signum, _):
        raise CollectionError('on-policy.stability-interrupted', f'Coordinator received signal {signum}')

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        (prepare if args.command == 'prepare' else train)(args)
    except Exception as error:
        print(dict(status='failed', code='on-policy.stability-command-failed', severity='error', stage=args.command,
                   message=str(error) or type(error).__name__), flush=True)
        raise
