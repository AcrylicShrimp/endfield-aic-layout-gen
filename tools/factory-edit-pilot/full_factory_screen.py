"""Fresh full-factory policy learning and equal-budget frozen controls."""
import argparse
from collections import Counter
import gzip
import hashlib
import json
from pathlib import Path
import platform
import re
import subprocess
import time

import torch

from full_factory_environment import FactoryRuntime, FactoryEpisode, ROOT, digest
from full_factory_policy import FullFactoryPolicy, SCHEMA, observe

TRAIN_ROOTS = ('chain-battery-38', 'chain-citron-48', 'chain-iron-nine-108', 'closure-refined')
SEEDS = (17, 29, 43)
EVAL_SEEDS = (1170000, 1180000)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write(path, value):
    path.write_text(json.dumps(value, indent=2)+'\n')


def save_gzip(path, value):
    with gzip.open(path, 'wt') as stream:
        json.dump(value, stream)


def advantages(scores):
    return [sum((s < other)-(s > other) for other in scores)/(len(scores)-1) for s in scores]


def geometry_action(w, actions):
    from full_factory_selection import geometry_choice
    return geometry_choice(w, actions, [True]*len(actions))


def run_episode(runtime, case, policy, method, sample_seed, args, training=False):
    episode = FactoryEpisode(runtime, case, args.horizon, args.episode_seconds, args.route_work)
    rng = torch.Generator(device='cpu').manual_seed(sample_seed)
    logps, entropies = [], []
    observation_seconds = inference_seconds = 0
    while not episode.done:
        began = time.perf_counter()
        description = episode.describe()
        observation = observe(episode.state['current'], description, episode.state,
                              1-episode.selected/args.horizon) if policy is not None else None
        observation_seconds += time.perf_counter()-began
        if episode.done:
            break
        began = time.perf_counter()
        if policy is not None:
            with torch.set_grad_enabled(training):
                logits = policy(observation)
                distribution = torch.distributions.Categorical(logits=logits)
                index = int(torch.multinomial(distribution.probs.detach().cpu(), 1, generator=rng))
                logp, entropy = distribution.log_prob(torch.tensor(index, device=logits.device)), distribution.entropy()
        elif method == 'uniform':
            index = int(torch.randint(len(description['actions']), (1,), generator=rng))
        else:
            index = geometry_action(episode.state['current'], description['actions'])
        inference_seconds += time.perf_counter()-began
        if episode.done:
            break
        episode.step(description['actions'][index])
        if policy is not None and training:
            logps.append(logp)
            entropies.append(entropy)
    summary = episode.summary()
    summary.update(sample_seed=sample_seed, observation_seconds=observation_seconds, inference_seconds=inference_seconds,
                   statuses=dict(Counter(r['status'] for r in episode.trace)),
                   kinds=dict(Counter(r['action']['kind'] for r in episode.trace)),
                   diagnostics=dict(Counter(d['code'] for r in episode.trace for d in r['diagnostics'])),
                   route_expansions=sum(r['route_expansions'] for r in episode.trace),
                   unchanged=sum(r['unchanged'] for r in episode.trace), revisited=sum(r['revisited'] for r in episode.trace))
    return summary, episode.state['best'], logps, entropies


def run(args):
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    started = time.perf_counter()
    def guard():
        if time.perf_counter()-started > args.screen_seconds:
            raise TimeoutError('factory.screen-budget: retain partial results and checkpoints; no complete result')
    spec = json.loads(args.inputs.read_text())
    for row in spec['rows']:
        for key, filename in (('request','request.json'), ('initial','initial.json'), ('original','original-request.json')):
            if sha(row[key]) != row['files_sha256'][filename]:
                raise ValueError(f'factory.input-hash: {row[key]}')
    cases = list(spec['rows'])
    strong = dict(next(r for r in cases if r['id'] == 'closure-original'))
    strong.update(id='closure-strong', initial=str(args.strong.resolve()))
    cases.append(strong)
    args.output.mkdir()
    source_files = list(Path(__file__).parent.glob('full_factory*.py')) + [Path(__file__).with_name('full_factory_executor.mjs')]
    # Resolve static relative imports from the executor, including transitive kernels.
    pending = [Path(__file__).with_name('full_factory_executor.mjs')]
    visited = set()
    while pending:
        path = pending.pop().resolve()
        if path in visited:
            continue
        visited.add(path)
        for relative in re.findall(r"from\s+['\"](\.[^'\"]+)['\"]", path.read_text()):
            pending.append((path.parent/relative).resolve())
    source_files = sorted(set(p.resolve() for p in source_files) | visited)
    catalog_paths = json.loads(Path(spec['catalogs']).read_text())
    protocol = dict(schema_version=1, observation_schema=SCHEMA, training_roots=TRAIN_ROOTS, seeds=SEEDS,
                    eval_seeds=EVAL_SEEDS, depths=args.depths, groups=args.groups, horizon=args.horizon,
                    group_episodes=4, route_work=args.route_work, episode_seconds=args.episode_seconds,
                    screen_seconds=args.screen_seconds, learning_rate=.0003, entropy_coefficient=.01,
                    width=32, gradient_clip=.5, device=args.device, python=platform.python_version(),
                    torch=torch.__version__, node=subprocess.check_output(['node','--version'],text=True).strip(),
                    catalogs={k:dict(path=p,sha256=sha(p)) for k,p in catalog_paths.items()}, inputs_sha256=sha(args.inputs), strong_sha256=sha(args.strong),
                    native_sha256=sha(args.native_library), cases=cases,
                    source_sha256={str(p.relative_to(ROOT)):sha(p) for p in source_files})
    write(args.output/'protocol.json', protocol)
    runtime = FactoryRuntime(args.native_library, spec['catalogs'])
    write(args.output/'native-build.json', runtime.build)
    rows, groups = [], []
    def retain(summary, best, name):
        directory = args.output/name
        directory.mkdir()
        save_gzip(directory/'episode.json.gz', summary)
        save_gzip(directory/'best.json.gz', best)
        row = {k:v for k,v in summary.items() if k != 'trace'}
        row['directory'] = name
        rows.append(row)
    def evaluate(policy, method, label):
        before = digest({k:v.detach().cpu().tolist() for k,v in policy.state_dict().items()}) if policy is not None else None
        for case in cases:
            for seed in EVAL_SEEDS:
                guard()
                result, best, _, _ = run_episode(runtime, case, policy, method, seed, args)
                result.update(arm=label, phase='evaluation')
                retain(result, best, f'{label}--{case["id"]}--{seed}')
        after = digest({k:v.detach().cpu().tolist() for k,v in policy.state_dict().items()}) if policy is not None else None
        if before != after:
            raise ValueError('factory.frozen-policy-mutated')
        print(json.dumps(dict(event='evaluation', arm=label, rows=len(rows), seconds=time.perf_counter()-started)), flush=True)
    try:
        for depth in args.depths:
            for seed in SEEDS:
                guard()
                torch.manual_seed(seed)
                policy = FullFactoryPolicy(depth).to(args.device)
                optimizer = torch.optim.Adam(policy.parameters(), lr=.0003)
                label = f'd{depth}-s{seed}'
                def checkpoint(name, next_group):
                    torch.save(dict(schema=SCHEMA, depth=depth, width=32, seed=seed, next_group=next_group,
                                    policy=policy.state_dict(), optimizer=optimizer.state_dict(), rng=torch.get_rng_state(),
                                    cuda_rng=torch.cuda.get_rng_state_all() if args.device == 'cuda' else [],
                                    protocol_sha256=sha(args.output/'protocol.json')), args.output/f'{label}-{name}.pt')
                checkpoint('untrained', 0)
                evaluate(policy, 'policy', label+'-untrained')
                for group in range(args.groups):
                    guard()
                    case = next(r for r in cases if r['id'] == TRAIN_ROOTS[group % len(TRAIN_ROOTS)])
                    scores, terms, entropy_terms = [], [], []
                    for attempt in range(4):
                        guard()
                        result, best, logps, entropies = run_episode(runtime, case, policy, 'policy',
                                                   seed*100000+group*4+attempt, args, training=True)
                        if result['selected'] != args.horizon:
                            raise TimeoutError('factory.training-episode-budget: incomplete group retained, no update')
                        result.update(arm=label, phase='training', group=group)
                        retain(result, best, f'{label}-train-{group}-{attempt}')
                        scores.append(tuple(result['best_metrics']['score']))
                        terms.append(torch.stack(logps).sum()/args.horizon)
                        entropy_terms.extend(entropies)
                    ranks = advantages(scores)
                    loss = -(torch.stack(terms)*torch.tensor(ranks, device=args.device)).mean()-.01*torch.stack(entropy_terms).mean()
                    optimizer.zero_grad()
                    loss.backward()
                    gradient_norm = float(torch.nn.utils.clip_grad_norm_(policy.parameters(), .5))
                    if not math_isfinite(float(loss), gradient_norm):
                        raise ValueError('factory.nonfinite-gradient')
                    optimizer.step()
                    record = dict(arm=label, group=group, case=case['id'], scores=scores, ranks=ranks,
                                  loss=float(loss.detach()), gradient_norm=gradient_norm)
                    groups.append(record)
                    with (args.output/'groups.jsonl').open('a') as stream:
                        stream.write(json.dumps(record)+'\n')
                    checkpoint('progress', group+1)
                    if (group+1) % 8 == 0:
                        print(json.dumps(dict(event='training', arm=label, group=group+1, seconds=time.perf_counter()-started)), flush=True)
                checkpoint('final', args.groups)
                evaluate(policy, 'policy', label+'-final')
        for method in ('uniform', 'geometry'):
            evaluate(None, method, method)
        report = dict(status='complete', schema_version=1, seconds=time.perf_counter()-started,
                      protocol_sha256=sha(args.output/'protocol.json'), rows=rows, groups=groups)
        write(args.output/'screen.json', report)
    except BaseException as error:
        write(args.output/'partial.json', dict(status='incomplete', error=str(error), rows=rows, groups=groups,
                                               seconds=time.perf_counter()-started))
        raise
    finally:
        runtime.close()
    print(json.dumps(dict(status='complete', rows=len(rows), output=str(args.output), seconds=report['seconds'])))


def math_isfinite(*values):
    import math
    return all(math.isfinite(v) for v in values)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--strong', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--depths', nargs='+', type=int, choices=(0,4), default=[0,4])
    parser.add_argument('--groups', type=int, default=32)
    parser.add_argument('--horizon', type=int, default=8)
    parser.add_argument('--route-work', type=int, default=5000)
    parser.add_argument('--episode-seconds', type=float, default=30)
    parser.add_argument('--screen-seconds', type=float, default=1800)
    parser.add_argument('--device', choices=('cpu','cuda'), default='cpu')
    args = parser.parse_args()
    if min(args.groups,args.horizon,args.route_work,args.episode_seconds,args.screen_seconds) <= 0:
        parser.error('All resource bounds must be positive')
    run(args)
