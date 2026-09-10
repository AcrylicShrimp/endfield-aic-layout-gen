"""Compare collection concurrency against independently audited group transitions."""
import argparse
import gc
import hashlib
import json
from pathlib import Path
import signal
import time

PROCESS_BEGAN = time.monotonic()

import torch

from full_factory_curriculum import source_manifest
from full_factory_large_learning import unpack, verify_sources
from full_factory_on_policy import parameter_hash, relative_returns, replay_gradients
from full_factory_on_policy_collection import CollectionError, ProcessTreeMemory, collect_group, read, sha, write
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy, SCHEMA


def tree_hash(value):
    """Canonical exact model/Adam/RNG identity, independent of torch ZIP metadata."""
    result = hashlib.sha256()
    def visit(item):
        if isinstance(item, torch.Tensor):
            result.update(json.dumps(['tensor', str(item.dtype), list(item.shape)]).encode())
            result.update(item.detach().cpu().contiguous().numpy().tobytes())
        elif isinstance(item, dict):
            result.update(b'dict[')
            for key in sorted(item, key=lambda k: (type(k).__name__, str(k))):
                visit(key)
                visit(item[key])
            result.update(b']')
        elif isinstance(item, (tuple, list)):
            result.update(type(item).__name__.encode()+b'[')
            for child in item:
                visit(child)
            result.update(b']')
        else:
            result.update(json.dumps([type(item).__name__, item], allow_nan=False).encode())
    visit(value)
    return result.hexdigest()


def semantic_episode(directory):
    """Drop only measured clocks; retain every action, diagnostic and physical state."""
    directory = Path(directory)
    episode = unpack(directory/'episode.json.gz')
    for field in ('elapsed_seconds', 'observation_seconds', 'selection_seconds', 'recording_seconds'):
        episode.pop(field)
    episode['config'].pop('episode_seconds')
    for row in episode['trace']:
        for field in ('seconds', 'elapsed_seconds', 'executor_seconds'):
            row.pop(field, None)
        if 'acceptance' in row:
            for field in ('elapsed_ms', 'validation_ms'):
                row['acceptance'].pop(field, None)
    return dict(episode=tree_hash(episode), states=tree_hash(unpack(directory/'states.json.gz')),
                best=tree_hash(unpack(directory/'best.json.gz')))


def load_policy_optimizer(protocol, checkpoint_path):
    checkpoint = torch.load(checkpoint_path, map_location='cpu', weights_only=False)
    if checkpoint['schema'] != SCHEMA or (checkpoint['depth'], checkpoint['width']) != (protocol['depth'], protocol['width']):
        raise ValueError('on-policy.collection-checkpoint-schema')
    model = FullFactoryPolicy(protocol['depth'], protocol['width'])
    model.load_state_dict(checkpoint['policy'], strict=True)
    if parameter_hash(model) != checkpoint['policy_sha256']:
        raise ValueError('on-policy.collection-checkpoint-policy')
    optimizer = torch.optim.Adam(model.parameters(), lr=protocol['config']['learning_rate'])
    optimizer.load_state_dict(checkpoint['optimizer'])
    torch.set_rng_state(checkpoint['rng'])
    return model, optimizer, checkpoint


def update_group(protocol, case, checkpoint_path, rows, guard):
    """One ordered recomputation and optimizer step after a complete group."""
    if len(rows) != 4 or any(not r['budget_complete'] or r['selected'] != protocol['config']['horizon'] for r in rows):
        raise CollectionError('on-policy.group-incomplete', 'Refusing an update from incomplete episodes')
    model, optimizer, before = load_policy_optimizer(protocol, checkpoint_path)
    returns = relative_returns([row['best_metrics']['score'] for row in rows])
    optimizer.zero_grad(set_to_none=True)
    gradient = replay_gradients(protocol, case, model, rows, returns, guard, unpack)
    guard()
    clipped = float(torch.nn.utils.clip_grad_norm_(model.parameters(), protocol['config']['gradient_clip']))
    if not torch.isfinite(torch.tensor(clipped)):
        raise ValueError('on-policy.collection-nonfinite-gradient')
    optimizer.step()
    guard()
    after = dict(before, policy=model.state_dict(), optimizer=optimizer.state_dict(), rng=torch.get_rng_state(),
                 next_group=before['next_group']+1, policy_sha256=parameter_hash(model))
    return returns, dict(gradient, pre_clip_gradient_norm=clipped), after


def config_check(config):
    groups = config.get('groups')
    if (config.get('schema_version') != 1 or not isinstance(groups, list) or not 1 <= len(groups) <= 4
            or any(type(g) is not int or not 0 <= g < 1024 for g in groups) or len(set(groups)) != len(groups)
            or config.get('initializations') != ['random', 'imitation-warm']
            or config.get('workers') != [1, 2, 4] or type(config.get('repeats')) is not int or not 1 <= config['repeats'] <= 3):
        raise ValueError('on-policy.collection-study-config: expected the registered group/worker matrix')
    for name, ceiling in [('episode_seconds', 300), ('group_seconds', 1200), ('study_seconds', 14400),
                          ('sampled_rss_bytes', 12*1024**3)]:
        if type(config.get(name)) is not int or not 0 < config[name] <= ceiling:
            raise ValueError(f'on-policy.collection-study-config-bound: {name}')


def prepare(args):
    config = read(args.config)
    config_check(config)
    parent, result = load_result(args.training)
    proof = read(args.training/'training-audit.json')
    checkpoint_proof = read(args.training/'checkpoint-audit.json')
    if (proof['status'] != 'passed' or proof['report_sha256'] != sha(args.training/'training-result.json')
            or proof['checkpoint_audit_sha256'] != sha(args.training/'checkpoint-audit.json')
            or checkpoint_proof['status'] != 'passed' or parent['config']['horizon'] != 32
            or parent['config']['group_episodes'] != 4):
        raise ValueError('on-policy.collection-study-parent-audit')
    conditions = []
    files = [args.config, *(args.training/n for n in ('protocol.json', 'training-result.json', 'training-audit.json', 'checkpoint-audit.json'))]
    for arm in config['initializations']:
        for group in config['groups']:
            record = next(g for g in result['groups'] if g['initialization'] == arm and g['group'] == group)
            before = args.training/arm/('initial.pt' if group == 0 else f'group-{group-1:04d}/checkpoint.pt')
            after = Path(record['checkpoint'])
            rows = sorted((r for r in result['rows'] if r['initialization'] == arm and r['group'] == group), key=lambda r: r['attempt'])
            if len(rows) != 4 or [r['bank'] for r in rows] != record['sample_seeds']:
                raise ValueError('on-policy.collection-study-parent-membership')
            for row in rows:
                audited = next(r for r in proof['rows'] if r['key'] == f'{arm}:{group}:{row["attempt"]}')
                for name in ('episode', 'states', 'best'):
                    file = Path(row['directory'])/f'{name}.json.gz'
                    if sha(file) != audited[f'{name}_sha256']:
                        raise ValueError(f'on-policy.collection-study-parent-artifact: {file}')
                    files.append(file)
            before_value = torch.load(before, map_location='cpu', weights_only=False)
            after_value = torch.load(after, map_location='cpu', weights_only=False)
            if (before_value['policy_sha256'] != record['policy_before_sha256']
                    or after_value['policy_sha256'] != record['policy_after_sha256'] or sha(after) != record['checkpoint_sha256']):
                raise ValueError('on-policy.collection-study-parent-checkpoint')
            conditions.append(dict(id=f'{arm}-group-{group:04d}', initialization=arm, group=group, case=record['case'],
                                   attempts=[dict(attempt=r['attempt'], seed=r['bank']) for r in rows],
                                   before=str(before.resolve()), after=str(after.resolve()),
                                   policy_sha256=record['policy_before_sha256'], expected_returns=record['returns'],
                                   expected_checkpoint=tree_hash(after_value),
                                   expected_episodes=[semantic_episode(r['directory']) for r in rows]))
            files.extend([before, after])
    sources = dict(parent['sources'])
    sources.update({str(f.resolve()): sha(f) for f in files})
    protocol = dict(parent, experiment='audited-on-policy-group-collection-concurrency', study=config,
                    parent=str(args.training.resolve()), parent_protocol_sha256=sha(args.training/'protocol.json'),
                    sources=sources, source_sha256=source_manifest(), conditions=conditions,
                    scope='Repeat independently audited transitions with identical behavior policies and seeds; no accumulated new training.')
    verify_sources(protocol)
    args.output.mkdir()
    write(args.output/'protocol.json', protocol)
    print(json.dumps(dict(status='frozen',conditions=len(conditions),groups=len(conditions)*config['repeats']*3,
                          selected_actions=len(conditions)*config['repeats']*3*4*32)), flush=True)


def run(args):
    began = PROCESS_BEGAN
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    protocol = read(args.screen/'protocol.json')
    config = protocol['study']
    config_check(config)
    verify_sources(protocol)
    if any((args.screen/name).exists() for name in ('progress.json', 'report.json', 'partial.json')):
        raise ValueError('on-policy.collection-study-existing: use a fresh output directory')
    rows = []
    active = None
    try:
        for ordinal, condition in enumerate(protocol['conditions']):
            case = next(c for c in protocol['cases'] if c['id'] == condition['case'])
            for repetition in range(config['repeats']):
                order = config['workers'][ordinal%3:] + config['workers'][:ordinal%3]
                if repetition:
                    order = list(reversed(order))
                for workers in order:
                    active = dict(condition=condition['id'], repetition=repetition, workers=workers)
                    directory = args.screen/f'{condition["id"]}-repeat-{repetition}-workers-{workers}'
                    gc.collect()
                    group_began = time.monotonic()
                    with ProcessTreeMemory() as memory:
                        def guard():
                            if memory.errors:
                                raise CollectionError('on-policy.collection-memory-measurement', str(memory.errors))
                            if (time.monotonic()-began >= config['study_seconds']
                                    or time.monotonic()-group_began >= config['group_seconds']
                                    or memory.peak_bytes > config['sampled_rss_bytes']):
                                raise CollectionError('on-policy.parallel-resource-bound', f'Study/group/sampled-RSS guard: {active}')
                        guard()
                        episode_rows, collection = collect_group(
                            args.screen/'protocol.json', condition['before'], condition['case'], condition['attempts'], directory,
                            group=condition['group'], workers=workers, group_seconds=config['group_seconds'],
                            policy_sha256=condition['policy_sha256'], episode_seconds=config['episode_seconds'],
                            rss_limit_bytes=config['sampled_rss_bytes'], guard=guard, memory=memory)
                        collected = time.monotonic()
                        returns, gradient, checkpoint = update_group(protocol, case, condition['before'], episode_rows, guard)
                        torch.save(checkpoint, directory/'checkpoint.pt')
                        updated = time.monotonic()
                        execution_memory = memory.report()
                        semantics = [semantic_episode(r['directory']) for r in episode_rows]
                        if (returns != condition['expected_returns'] or semantics != condition['expected_episodes']
                                or tree_hash(checkpoint) != condition['expected_checkpoint']):
                            write(directory/'mismatch.json', dict(returns=returns, episodes=semantics,
                                                                 checkpoint=tree_hash(checkpoint), expected=condition))
                            raise CollectionError('on-policy.collection-semantic-mismatch', f'Audited transition differs: {directory}')
                        guard()
                    if memory.errors:
                        raise CollectionError('on-policy.collection-memory-measurement', str(memory.errors))
                    record = dict(**active, case=condition['case'], instances=case['instances'], selected=sum(r['selected'] for r in episode_rows),
                                  collection_seconds=collected-group_began, replay_update_checkpoint_seconds=updated-collected,
                                  execution_group_seconds=updated-group_began, verification_seconds=time.monotonic()-updated,
                                  total_group_seconds=time.monotonic()-group_began, memory=memory.report(), gradient=gradient,
                                  execution_memory=execution_memory,
                                  returns=returns, semantic_identity=True, checkpoint_sha256=sha(directory/'checkpoint.pt'),
                                  completion_order=collection['completion_order'], directory=str(directory.resolve()))
                    record['selected_per_execution_second'] = record['selected']/record['execution_group_seconds']
                    rows.append(record)
                    write(directory/'group.json', record)
                    write(args.screen/'progress.json', dict(status='incomplete', rows=rows, active=active))
                    print(json.dumps({k:record[k] for k in ('condition','repetition','workers','execution_group_seconds','selected_per_execution_second','semantic_identity')}), flush=True)
        verify_sources(protocol)
        write(args.screen/'report.json', dict(schema_version=1, status='complete', protocol_sha256=sha(args.screen/'protocol.json'),
                                             seconds=time.monotonic()-began, rows=rows,
                                             selected=sum(r['selected'] for r in rows), training_accumulated=False))
    except BaseException as error:
        diagnostic = error.diagnostic if isinstance(error, CollectionError) else dict(
            code='on-policy.collection-study-failed', severity='error', stage='on-policy-collection-study', message=str(error))
        write(args.screen/'partial.json', dict(status='incomplete', active=active, diagnostic=diagnostic, rows=rows))
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    freeze = sub.add_parser('prepare')
    for name in ('training', 'config', 'output'):
        freeze.add_argument('--'+name, required=True, type=Path)
    sub.add_parser('run').add_argument('--screen', required=True, type=Path)
    args = parser.parse_args()
    def interrupted(signum, _):
        raise CollectionError('on-policy.collection-study-interrupted', f'Coordinator received signal {signum}')
    signal.signal(signal.SIGTERM, interrupted)
    (prepare if args.command == 'prepare' else run)(args)
