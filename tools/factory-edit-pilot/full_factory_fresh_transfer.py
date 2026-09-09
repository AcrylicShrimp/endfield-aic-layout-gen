"""Freeze audited policies and compare complete fresh factories without training."""
import argparse
from collections import Counter
from pathlib import Path
import time

import torch

from full_factory_curriculum import source_manifest
from full_factory_decoding import TemperaturePolicy
from full_factory_environment import FactoryRuntime
from full_factory_landscape import episode
from full_factory_large_learning import read, verify_sources
from full_factory_policy import SCHEMA, FullFactoryPolicy
from full_factory_screen import sha, write, save_gzip


def validate_config(config):
    if config.get('schema_version') != 1:
        raise ValueError('fresh-transfer.config-schema')
    for name, cap in [('horizon', 64), ('episode_seconds', 300), ('route_work', 100000), ('screen_seconds', 7200)]:
        if type(config.get(name)) is not int or not 0 < config[name] <= cap:
            raise ValueError(f'fresh-transfer.config-bound: {name}')
    banks = config.get('banks')
    if not isinstance(banks, list) or not banks or len(set(banks)) != len(banks) or any(type(b) is not int or b < 0 for b in banks):
        raise ValueError('fresh-transfer.config-banks')
    if type(config.get('temperature')) not in (int, float) or not 0 < config['temperature'] <= 1:
        raise ValueError('fresh-transfer.config-temperature')


def prepare(args):
    config = read(args.config)
    validate_config(config)
    admission, inputs = read(args.admission/'report.json'), read(args.inputs)
    if admission['status'] != 'measured' or admission['attempted'] != len(admission['rows']):
        raise ValueError('fresh-transfer.incomplete-admission')
    accepted = [r for r in admission['rows'] if r['status'] == 'accepted' and r['factory_validated']]
    if not accepted or len(accepted) != admission['accepted'] or [r['id'] for r in accepted] != [c['id'] for c in inputs['rows']]:
        raise ValueError('fresh-transfer.accepted-membership')
    sources = {str(Path(item['path']).resolve()): item['sha256'] for item in admission['provenance'] + inputs['provenance']}
    cases = []
    for row, case in zip(accepted, inputs['rows']):
        for key, name in [('initial', 'initial.json'), ('request', 'request.json'), ('original', 'original-request.json')]:
            if sha(case[key]) != case['files_sha256'][name]:
                raise ValueError(f'fresh-transfer.input-hash: {case[key]}')
            sources[case[key]] = case['files_sha256'][name]
        cases.append(dict(case, family=row['family'], split='development', size_stratum=row['size_stratum'],
            transfer_group='fresh-trained-family' if row['family'] == 'enriched-consumables' else 'new-target-graph',
            internal_pipe_requirements=row['internal_pipe_requirements'], topology=row['topology']))
    checkpoints, parents = {}, []
    for label, directory in [('exposure', args.exposure), ('retention', args.retention)]:
        p, r, a = (read(directory/n) for n in ['protocol.json', 'report.json', 'audit.json'])
        if r['status'] != 'complete' or a['status'] != 'passed' or a['report_sha256'] != sha(directory/'report.json') or r['protocol_sha256'] != sha(directory/'protocol.json'):
            raise ValueError(f'fresh-transfer.parent-audit: {directory}')
        verify_sources(p)
        sources.update(p['sources'])
        for name in ['protocol.json', 'report.json', 'audit.json']:
            sources[str((directory/name).resolve())] = sha(directory/name)
        for seed in [17, 29]:
            name, file = f'learned-{seed}', directory/f'learned-{seed}.pt'
            if sha(file) != a['checkpoints'][name]:
                raise ValueError(f'fresh-transfer.checkpoint-audit: {file}')
            c = torch.load(file, map_location='cpu', weights_only=False)
            if c['protocol_sha256'] != sha(directory/'protocol.json') or c['step'] != p['config']['updates'] or c['seed'] != seed or c['schema'] != SCHEMA or (c['depth'], c['width']) != (p['depth'], p['width']):
                raise ValueError(f'fresh-transfer.checkpoint-identity: {file}')
            checkpoints[f'{label}-{seed}'] = dict(path=str(file.resolve()), sha256=sha(file), protocol_sha256=c['protocol_sha256'])
            sources[str(file.resolve())] = sha(file)
        parents.append(p)
    left, right = parents
    for key in ['checkpoint', 'schema', 'depth', 'width', 'native_library']:
        if left[key] != right[key]:
            raise ValueError(f'fresh-transfer.parent-contract: {key}')
    catalogs = {k: sha(v) for k, v in read(inputs['catalogs']).items()}
    for p in parents:
        if catalogs != {k: sha(v) for k, v in read(p['catalogs']).items()}:
            raise ValueError('fresh-transfer.catalog-drift')
    parent = torch.load(right['checkpoint'], map_location='cpu', weights_only=False)
    if parent['schema'] != SCHEMA or parent['protocol_sha256'] != sha(Path(right['parent'])/'protocol.json'):
        raise ValueError('fresh-transfer.pretrained-identity')
    checkpoints = dict(parent=dict(path=right['checkpoint'], sha256=sha(right['checkpoint']), protocol_sha256=parent['protocol_sha256']), **checkpoints)
    files = [args.config, args.inputs, args.admission/'report.json', Path(inputs['catalogs'])]
    sources.update({str(f.resolve()): sha(f) for f in files})
    protocol = dict(schema_version=1, experiment='fresh-factory-frozen-transfer', config=config, cases=cases,
        admission=str(args.admission.resolve()), inputs=str(args.inputs.resolve()),
        parent_screens=dict(exposure=str(args.exposure.resolve()), retention=str(args.retention.resolve())),
        catalogs=inputs['catalogs'], native_library=right['native_library'], checkpoints=checkpoints,
        methods=[*checkpoints, 'geometry'], schema=SCHEMA, depth=right['depth'], width=right['width'],
        sources=sources, source_sha256=source_manifest(), training_performed=False,
        scope='Prospective new target graphs and fresh sizes; constituent recipe overlap retained and no sealed-family claim')
    verify_sources(protocol)
    args.output.mkdir()
    write(args.output/'protocol.json', protocol)
    print(dict(status='frozen', attempted=admission['attempted'], accepted=len(cases), episodes=len(cases)*len(protocol['methods'])*len(config['banks'])), flush=True)


def run(args):
    began = time.perf_counter()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    p = read(args.screen/'protocol.json')
    validate_config(p['config'])
    verify_sources(p)
    if any((args.screen/n).exists() for n in ['report.json', 'progress.json', 'partial.json']):
        raise ValueError('fresh-transfer.existing-evaluation: use a fresh screen')
    config, rows = p['config'], []
    try:
        for method in p['methods']:
            model = None
            if method != 'geometry':
                record = p['checkpoints'][method]
                if sha(record['path']) != record['sha256']:
                    raise ValueError(f'fresh-transfer.checkpoint-drift: {method}')
                checkpoint = torch.load(record['path'], map_location='cpu', weights_only=False)
                if checkpoint['schema'] != p['schema'] or checkpoint['protocol_sha256'] != record['protocol_sha256']:
                    raise ValueError(f'fresh-transfer.checkpoint-contract: {method}')
                model = FullFactoryPolicy(p['depth'], p['width'])
                model.load_state_dict(checkpoint['policy'])
                model.eval()
                frozen = {k: v.clone() for k, v in model.state_dict().items()}
            for case in p['cases']:
                for bank in config['banks']:
                    if time.perf_counter()-began >= config['screen_seconds']:
                        raise TimeoutError('fresh-transfer.study-budget')
                    runtime = FactoryRuntime(p['native_library'], p['catalogs'])
                    try:
                        if runtime.description_cache or runtime.description_cache_bytes:
                            raise ValueError('fresh-transfer.nonempty-runtime-cache')
                        arm = dict(horizon=config['horizon'], balanced=False, memory=True, geometry=method == 'geometry')
                        actor = TemperaturePolicy(model, config['temperature']) if model is not None else None
                        bounded = argparse.Namespace(episode_seconds=config['episode_seconds'], route_work=config['route_work'])
                        summary, best, _, _ = episode(runtime, case, actor, arm, bank, bounded)
                        directory = args.screen/f'{method}--{case["id"]}--{bank}'
                        directory.mkdir()
                        save_gzip(directory/'episode.json.gz', summary)
                        save_gzip(directory/'best.json.gz', best)
                        rows.append(dict(method=method, case=case['id'], family=case['family'], split=case['split'],
                            transfer_group=case['transfer_group'], instances=case['instances'], bank=bank, directory=str(directory.resolve()),
                            initial_metrics=summary['initial_metrics'], best_metrics=summary['best_metrics'], selected=summary['selected'],
                            seconds=summary['total_seconds'], observation_seconds=summary['observation_seconds'], selection_seconds=summary['selection_seconds'],
                            kinds=summary['kinds'], statuses=summary['statuses'], route_expansions=summary['route_expansions'],
                            improvements=dict(Counter(t['action']['kind'] for t in summary['trace'] if t['status'] == 'improved')),
                            budget_complete=summary['selected'] == config['horizon'] and not any(t['status'] == 'late' for t in summary['trace'])))
                        write(args.screen/'progress.json', dict(status='incomplete', rows=rows))
                    finally:
                        runtime.close()
                print(dict(method=method, case=case['id'], episodes=len(rows), score=rows[-1]['best_metrics']['score'], seconds=time.perf_counter()-began), flush=True)
            if model is not None and any(not torch.equal(v, frozen[k]) for k, v in model.state_dict().items()):
                raise ValueError('fresh-transfer.policy-mutated')
        verify_sources(p)
        write(args.screen/'report.json', dict(schema_version=1, status='complete', protocol_sha256=sha(args.screen/'protocol.json'),
            seconds=time.perf_counter()-began, policy_immutable=True, training_performed=False, rows=rows))
    except BaseException as error:
        write(args.screen/'partial.json', dict(status='incomplete', code='fresh-transfer.failed', severity='error', stage='evaluation', message=str(error), rows=rows))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    freeze = sub.add_parser('prepare')
    for name in ['admission', 'inputs', 'exposure', 'retention', 'config', 'output']:
        freeze.add_argument('--'+name, required=True, type=Path)
    evaluate = sub.add_parser('run')
    evaluate.add_argument('--screen', required=True, type=Path)
    args = parser.parse_args()
    (prepare if args.command == 'prepare' else run)(args)
