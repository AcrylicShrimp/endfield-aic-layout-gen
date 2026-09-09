"""Reproduce and intervene on single audited zero-reward-gradient Adam updates."""
import argparse
from collections import Counter
from copy import deepcopy
import math
from pathlib import Path
import time

import torch

from full_factory_environment import FactoryRuntime, digest
from full_factory_large_learning import read, unpack, verify_sources
from full_factory_learnability_audit import observation_key
from full_factory_on_policy import parameter_hash, replay_gradients
from full_factory_on_policy_result import load_result
from full_factory_policy import FullFactoryPolicy, observe
from full_factory_screen import sha, write
from full_factory_selection import SelectionMemory, action_key, physical_key, distribution


def require(ok, entity):
    if not ok:
        raise ValueError(f'on-policy.counterfactual-contract: {entity}')


def identical(a, b):
    if isinstance(a, torch.Tensor):
        return isinstance(b, torch.Tensor) and a.dtype == b.dtype and torch.equal(a, b)
    if isinstance(a, dict):
        return isinstance(b, dict) and a.keys() == b.keys() and all(identical(a[k], b[k]) for k in a)
    if isinstance(a, (list, tuple)):
        return type(a) is type(b) and len(a) == len(b) and all(identical(x, y) for x, y in zip(a, b))
    return a == b


def check_baseline(model, optimizer, expected):
    require(identical(model.state_dict(), expected['policy']), 'baseline policy mismatch')
    require(identical(optimizer.state_dict(), expected['optimizer']), 'baseline Adam mismatch')


def branch_step(model, optimizer, gradients, branch, clip):
    """Operate only on caller-owned model/optimizer; zero and skip are distinct."""
    if branch['skip']:
        return None
    if branch['reset_adam']:
        optimizer.state.clear()
    for group in optimizer.param_groups:
        group['lr'] *= branch['lr_scale']
    parameters = list(model.parameters())
    require(len(parameters) == len(gradients), 'gradient count')
    for parameter, gradient in zip(parameters, gradients):
        require(parameter.shape == gradient.shape, 'gradient shape')
        parameter.grad = gradient.detach().clone() * branch['entropy_scale']
    norm = float(torch.nn.utils.clip_grad_norm_(parameters, clip))
    require(math.isfinite(norm), 'gradient norm')
    optimizer.step()
    require(all(torch.isfinite(v).all() for v in model.parameters()), 'nonfinite branch')
    return norm


def config_check(c):
    require(c['schema_version'] == 1 and c['initialization'] == 'imitation-warm', 'configuration version/arm')
    require(c['groups'] == [0, 38], 'registered groups')
    require(type(c['wall_seconds']) is int and 0 < c['wall_seconds'] <= 900, 'wall budget')
    require(1 <= len(c['branches']) <= 16, 'branch count')
    names = [b['name'] for b in c['branches']]
    require(len(set(names)) == len(names) and names[0] == 'baseline', 'branch names/order')
    for b in c['branches']:
        require(isinstance(b['name'], str) and b['name'], 'branch name')
        for key in ('entropy_scale', 'lr_scale'):
            require(type(b[key]) in (int, float) and math.isfinite(b[key]) and 0 <= b[key] <= 1, key)
        require(b['lr_scale'] > 0, 'positive learning rate')
        require(type(b['reset_adam']) is bool and type(b['skip']) is bool, 'branch booleans')
    require(c['branches'][0] == dict(name='baseline', entropy_scale=1, reset_adam=False, lr_scale=1, skip=False),
            'unmodified baseline')


def run(args, context):
    started = time.perf_counter()
    c = read(args.config)
    config_check(c)

    def guard():
        if time.perf_counter() - started >= c['wall_seconds']:
            raise TimeoutError(f'on-policy.counterfactual-budget: {context["entity"]}')

    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    p, result = load_result(args.training)
    proof = read(args.training/'training-audit.json')
    checkpoints = read(args.training/'checkpoint-audit.json')
    for audit in (proof, checkpoints):
        require(audit['status'] == 'passed' and audit['report_sha256'] == sha(args.training/'training-result.json'), 'audit binding')
    require(proof['checkpoint_audit_sha256'] == sha(args.training/'checkpoint-audit.json'), 'checkpoint audit binding')
    diagnosis = read(args.diagnosis/'report.json')
    panel_protocol = read(args.diagnosis/'protocol.json')
    require(diagnosis['status'] == 'complete' and diagnosis['protocol_sha256'] == sha(args.diagnosis/'protocol.json'), 'diagnosis binding')
    require(panel_protocol['sources'][str((args.training/'training-result.json').resolve())] == sha(args.training/'training-result.json'),
            'diagnosis training binding')
    sources = {}

    def bind(path, expected=None):
        path = Path(path).resolve()
        actual = sha(path)
        require(expected is None or expected == actual, f'source hash: {path}')
        sources[str(path)] = actual

    for path in (args.config, Path(__file__), args.diagnosis/'protocol.json', args.diagnosis/'report.json'):
        bind(path)
    for name in ('protocol.json', 'training-result.json', 'training-audit.json', 'checkpoint-audit.json'):
        bind(args.training/name)
    checkpoint_hashes = {r['path']: r['sha256'] for r in checkpoints['rows']}
    groups = []
    arm = next(a for a in result['arms'] if a['initialization'] == c['initialization'])
    for number in c['groups']:
        group = next(g for g in result['groups'] if g['initialization'] == c['initialization'] and g['group'] == number)
        require(group['flat_returns'] and not any(group['returns']) and group['reward_gradient_norm'] == 0, f'nonflat group {number}')
        before = Path(arm['initial']) if number == 0 else args.training/c['initialization']/f'group-{number-1:04d}/checkpoint.pt'
        after = Path(group['checkpoint'])
        for path in (before, after):
            bind(path, checkpoint_hashes[str(path.resolve())])
        episodes = [r for r in result['rows'] if r['initialization'] == c['initialization'] and r['group'] == number]
        require([r['attempt'] for r in episodes] == list(range(p['config']['group_episodes'])), 'episode order')
        groups.append((group, before, after, episodes))
    panel = panel_protocol['panel']
    require(len(panel) == 10 and {r['case'] for r in panel} == set(p['training_cases']), 'panel membership')
    panel_episodes = []
    for row in panel:
        source = next(r for r in result['rows'] if r['initialization'] == 'random' and r['group'] == row['group'] and r['attempt'] == 0)
        require(source['directory'] == row['directory'] and source['case'] == row['case'], 'panel source identity')
        panel_episodes.append(source)
    for row in panel_episodes + [r for _, _, _, episodes in groups for r in episodes]:
        audited = next(r for r in proof['rows'] if r['key'] == f'{row["initialization"]}:{row["group"]}:{row["attempt"]}')
        for name in ('states', 'episode'):
            bind(Path(row['directory'])/f'{name}.json.gz', audited[f'{name}_sha256'])
    args.output.mkdir()
    write(args.output/'protocol.json', dict(schema_version=1, config=c, sources=sources,
        runtime_source_sha256=p['source_sha256'], panel=panel, action_selection_performed=False,
        scope='Single updates from identical behavior weights; no resumed training or rollout evaluation.'))
    guard()
    observations = []
    runtime = FactoryRuntime(p['native_library'], p['catalogs'])
    try:
        for source in panel:
            context['entity'] = source['case']
            guard()
            record = unpack(Path(source['directory'])/'states.json.gz')[0]
            episode = unpack(Path(source['directory'])/'episode.json.gz')
            state = record['state']
            require(digest(state['current']) == episode['trace'][0]['current_before_sha256'], 'panel current')
            description = runtime.rpc('describe', state['current'], options={})
            obs = observe(state['current'], description, state, 1)
            require(observation_key(obs) == record['observation_sha256'], 'panel observation')
            physical = physical_key(state['current'])
            keys = [action_key(a, description, state['current'], physical) for a in description['actions']]
            allowed, exhausted = SelectionMemory(state['current']).mask(physical, keys, description['actions'], p['config']['memory'])
            require(allowed == record['allowed'] and exhausted == episode['trace'][0]['memory_exhausted'], 'panel mask')
            observations.append((source, obs, description['actions'], allowed))
    finally:
        runtime.close()

    def fresh(before):
        model = FullFactoryPolicy(p['depth'], p['width'])
        model.load_state_dict(before['policy'], strict=True)
        optimizer = torch.optim.Adam(model.parameters(), lr=p['config']['learning_rate'])
        optimizer.load_state_dict(deepcopy(before['optimizer']))
        return model, optimizer

    def probabilities(model, observation, actions, allowed):
        with torch.no_grad():
            return distribution(model(observation)/p['config']['temperature'], actions, allowed, p['config']['balanced']).probs

    results = []
    for group, before_path, after_path, episodes in groups:
        context['entity'] = f'group-{group["group"]}:replay'
        guard()
        before = torch.load(before_path, map_location='cpu', weights_only=False)
        expected = torch.load(after_path, map_location='cpu', weights_only=False)
        model, _ = fresh(before)
        require(parameter_hash(model) == group['policy_before_sha256'], 'behavior policy')
        case = next(case for case in p['cases'] if case['id'] == group['case'])
        stats = replay_gradients(p, case, model, episodes, group['returns'], guard, unpack)
        require(stats['reward_gradient_norm'] == 0 and stats['entropy_gradient_norm'] == group['entropy_gradient_norm'], 'replayed gradient')
        gradients = [v.grad.detach().clone() for v in model.parameters()]
        original_probs = [probabilities(model, obs, actions, allowed) for _, obs, actions, allowed in observations]
        branch_results = []
        for branch in c['branches']:
            context['entity'] = f'group-{group["group"]}:{branch["name"]}'
            guard()
            candidate, optimizer = fresh(before)
            norm = branch_step(candidate, optimizer, gradients, branch, p['config']['gradient_clip'])
            if branch['name'] == 'baseline':
                check_baseline(candidate, optimizer, expected)
            if branch['skip']:
                check_baseline(candidate, optimizer, before)
            probes = []
            for (source, obs, actions, allowed), old in zip(observations, original_probs):
                guard()
                probs = probabilities(candidate, obs, actions, allowed)
                require(torch.isfinite(probs).all(), 'nonfinite probabilities')
                mass = Counter()
                for action, probability in zip(actions, probs.tolist()):
                    mass[action['kind']] += probability
                probes.append(dict(case=source['case'], total_variation=float((probs-old).abs().sum()/2), kind_probability=dict(mass)))
            delta = torch.cat([(v.detach().double()-before['policy'][k].double()).reshape(-1) for k, v in candidate.state_dict().items()])
            branch_results.append(dict(**branch, pre_clip_gradient_norm=norm, policy_sha256=parameter_hash(candidate),
                parameter_delta_l2=float(delta.norm()), maximum_absolute_parameter_delta=float(delta.abs().max()),
                mean_total_variation=sum(r['total_variation'] for r in probes)/len(probes),
                maximum_total_variation=max(r['total_variation'] for r in probes), probes=probes))
            print(dict(group=group['group'], branch=branch['name'], mean_tv=branch_results[-1]['mean_total_variation']), flush=True)
        results.append(dict(group=group['group'], case=group['case'], replay=stats, baseline_exact=True, branches=branch_results))
    guard()
    for path, expected_hash in sources.items():
        require(sha(path) == expected_hash, f'final source drift: {path}')
    verify_sources(p)
    guard()
    write(args.output/'report.json', dict(schema_version=1, status='complete', protocol_sha256=sha(args.output/'protocol.json'),
        groups=results, seconds=time.perf_counter()-started, action_selection_performed=False,
        scope='Causal mechanical effects on these two single updates; final factory-quality effects unmeasured.'))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('training', 'diagnosis', 'config', 'output'):
        parser.add_argument('--'+name, required=True, type=Path)
    args = parser.parse_args()
    if args.output.exists():
        parser.error(f'output already exists: {args.output}; use a fresh directory')
    context = dict(entity=str(args.training.resolve()))
    try:
        run(args, context)
    except Exception as error:
        failure = dict(schema_version=1, status='incomplete', diagnostics=[dict(code='on-policy.counterfactual-failed',
            severity='error', stage='counterfactual', entity=context['entity'], message=str(error) or type(error).__name__)])
        if args.output.is_dir():
            write(args.output/'partial.json', failure)
        print(failure, flush=True)
        raise
