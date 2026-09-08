"""Measure optimizer and calibration movement in one private, coupled PPO update."""

import argparse
from copy import deepcopy
import json
from pathlib import Path
import time

import numpy as np
import torch

from clock_intervention import paired_counts
from curriculum import Collector, FIELDS
from evaluation_batch import episodes
from evidence import write
from geometry_policy_evaluation import admitted_cases
from mastery_policy_selection import digest, read
from native import Runtime, check, layout, trace
from native_training_comparison import equal, policy_hash
from policy import batch, update
from prefix_recovery import replay
from reference.acceptance import Catalog, check as reference_check
from tree_search_pilot import freeze


def categorical_distance(reference, candidate):
    """Finite log probabilities over a single actual candidate domain."""
    if reference.shape != candidate.shape or reference.ndim != 1 or reference.numel() == 0:
        raise ValueError('update_stability.domain: matching nonempty categorical vectors required')
    if not torch.isfinite(reference).all() or not torch.isfinite(candidate).all():
        raise ValueError('update_stability.nonfinite: nonfinite categorical log probabilities')
    a, b = reference.double(), candidate.double()
    # Renormalize in double precision to avoid a tiny negative KL from float sums.
    a, b = a-a.logsumexp(0), b-b.logsumexp(0)
    return float((a.exp()*(a-b)).sum().clamp_min(0)), float((a.exp()-b.exp()).abs().sum()/2)


@torch.no_grad()
def distance(reference, candidate, rollout, minibatch):
    device = next(candidate.parameters()).device
    rows = []
    for start in range(0, len(rollout['observations']), minibatch):
        observations = rollout['observations'][start:start+minibatch]
        data = {k: v.to(device) for k, v in batch(observations).items()}
        a, av = reference(data)
        b, bv = candidate(data)
        a, b = a.log_softmax(-1).cpu(), b.log_softmax(-1).cpu()
        for index, observation in enumerate(observations):
            count = len(observation['candidates'])
            kl, tv = categorical_distance(a[index, :count], b[index, :count])
            action = rollout['actions'][start+index]
            rows.append((kl, tv, abs(float(b[index, action]-a[index, action])),
                         float(bv[index]-av[index])**2))
    if not rows or not np.isfinite(rows).all():
        raise ValueError('update_stability.nonfinite: empty or nonfinite policy-distance observations')
    values = np.asarray(rows)
    return {'observations': len(rows), 'exact_kl_mean': float(values[:, 0].mean()),
            'exact_kl_max': float(values[:, 0].max()), 'total_variation_mean': float(values[:, 1].mean()),
            'sampled_log_probability_absolute_mean': float(values[:, 2].mean()),
            'sampled_log_probability_absolute_max': float(values[:, 2].max()),
            'value_rms': float(np.sqrt(values[:, 3].mean()))}


def measured_update(model, optimizer, rollout, config, rng):
    """Observe the existing calibration boundary without changing update semantics."""
    anchor = deepcopy(model)
    original = model.calibrate_normalization
    captured = []

    def calibrate(observations, size):
        captured.append(deepcopy(model))
        # Do not copy this instance callback into the diagnostic snapshot.
        del captured[-1].calibrate_normalization
        return original(observations, size)

    if 'calibrate_normalization' in model.__dict__:
        raise ValueError('update_stability.instrumentation: calibration already overridden')
    model.calibrate_normalization = calibrate
    try:
        metrics = update(model, optimizer, rollout, config, rng)
    finally:
        del model.calibrate_normalization
    if len(captured) != 1:
        raise ValueError('update_stability.instrumentation: expected exactly one calibration pass')
    before = captured[0]
    metrics['deployment_distances'] = {
        'anchor_to_post_optimizer': distance(anchor, before, rollout, config['minibatch']),
        'anchor_to_post_calibration': distance(anchor, model, rollout, config['minibatch']),
        'post_optimizer_to_post_calibration': distance(before, model, rollout, config['minibatch'])}
    return metrics


def evaluate(actor, runtime, catalog, config, spec, output, case, condition):
    initial = list(runtime.new_engine().initial_score)
    records, layouts, traces = [], [], []
    for seed, env, total in episodes(actor, runtime, spec['episodes'], spec['horizon'],
                                    spec['sampling_seed_start'], spec['batch_size']):
        best = layout(env, 'best')
        native_result, independent = check(env, best), reference_check(catalog, best)
        if not native_result['valid'] or not independent['valid'] or native_result['score'] != independent['score']:
            raise ValueError('update_stability.acceptance: native/reference output mismatch')
        commands, counts = trace(env), json.loads(env.episode_json())['counts']
        if (len(commands) != spec['horizon'] or counts['commands'] != spec['horizon']
                or abs(total-(initial[0]-independent['score'][0])/initial[0]) > 1e-10):
            raise ValueError('update_stability.accounting: command or reward total differs')
        records.append({'seed': seed, 'score': independent['score'], 'counts': counts, 'return': total,
            'translated_facilities': [f['id'] for f in runtime.case['facilities']
                                      if best['placements'][f['id']][:2] != f['pose'][:2]]})
        layouts.append(best)
        traces.append(commands)
    order = sorted(range(len(records)), key=lambda i: (records[i]['score'], i))
    selected = {'best': order[0], 'median': order[len(order)//2]}
    replays = {}
    for role, index in selected.items():
        replays[role] = replay(catalog, config, [], traces[index], layouts[index])
        path = output/f'{case}-{condition}-{role}.json'
        write(path, {'seed': records[index]['seed'], 'trace': traces[index], 'layout': layouts[index]})
        replays[role].update(path=path.name, sha256=digest(path))
    return {'case': case, 'condition': condition, 'initial_score': initial, 'episodes': records,
        'translated_successes': sum(r['score'][0] < initial[0] and bool(r['translated_facilities']) for r in records),
        'median_score': records[selected['median']]['score'], 'best_score': records[selected['best']]['score'],
        'replays': replays}


def run(root, source, protocol, library, output):
    spec = read(protocol)
    if (spec.get('schema_version') != 1 or spec.get('purpose') != 'single_update_stability'
            or spec['width'] != 1 or spec['training_seed'] != 43 or spec['anchor_commands'] != 1179648
            or spec['learning_rate_multipliers'] != [0, 0.1, 0.3, 1]
            or spec['collection_seed'] != 1026000 or spec['shuffle_seed'] != 1027000
            or spec['collection_horizon'] != 16 or spec['collection_rounds'] != 4
            or spec['device'] != 'cpu' or spec['threads'] != 1 or spec['horizon'] != 128
            or spec['episodes'] != 16 or spec['sampling_seed_start'] != 1028000 or spec['batch_size'] != 16):
        raise ValueError('update_stability.protocol: unsupported diagnostic protocol')
    if output.exists():
        raise ValueError(f'update_stability.output: {output}: choose a fresh directory')
    cases = admitted_cases(root, read(root/spec['geometry_protocol']))['train']
    if len(cases) != 16:
        raise ValueError('update_stability.cases: expected sixteen admitted training roots')
    output.mkdir(parents=True)
    result = {'schema_version': 1, 'status': 'running', 'protocol': spec, 'protocol_sha256': digest(protocol),
              'branches': {}, 'cells': [], 'pairs': []}
    started, active = time.perf_counter(), None
    try:
        guard_dir = output/'source-guard'
        guard_dir.mkdir()
        anchor, config, guard = freeze(root, source, spec, cases, library, protocol, guard_dir)
        result['source_guard'] = guard
        directory = source/'width1-seed43'
        checkpoint = directory/f"checkpoint-{spec['anchor_commands']}.pt"
        audit = read(directory/'study-audit.json')
        if digest(checkpoint) != audit['checkpoints_sha256'][str(spec['anchor_commands'])]:
            raise ValueError('update_stability.checkpoint: anchor differs from audit')
        saved = torch.load(checkpoint, map_location='cpu', weights_only=False)
        if (saved['metadata']['completed_commands'] != spec['anchor_commands'] or not saved.get('optimizer')
                or not equal(saved['policy'], saved['evaluation_policy'])):
            raise ValueError('update_stability.checkpoint: missing Adam or unequal actor/learner state')
        anchor.load_state_dict(saved['policy'])
        initial_hash = policy_hash(anchor.state_dict())
        torch.save({'purpose': 'diagnostic_only', 'state_dict': anchor.state_dict()}, output/'anchor.pt')
        result['anchor'] = {'checkpoint_sha256': digest(checkpoint), 'policy_sha256': initial_hash,
                            'file_sha256': digest(output/'anchor.pt')}
        collection_config = {**config, 'horizon': spec['collection_horizon']}
        runtimes = {case: Runtime(root, path, collection_config, library) for case, path in cases.items()}
        collector = Collector(runtimes, collection_config, 16, spec['collection_seed'], True, threaded=False)
        rollout = {key: [] for key in FIELDS}
        collection_evidence = []
        try:
            for round_index in range(spec['collection_rounds']):
                part, _ = collector.collect(anchor, [0]*16)
                for key in FIELDS:
                    rollout[key].extend(part[key])
                for case, env in zip(collector.keys, collector.envs):
                    best, commands = layout(env, 'best'), trace(env)
                    catalog = Catalog(root, cases[case])
                    accepted = check(env, best)
                    if not accepted['valid'] or not reference_check(catalog, best)['valid']:
                        raise ValueError('update_stability.collection: invalid incumbent')
                    validation = replay(catalog, collection_config, [], commands, best)
                    collection_evidence.append({'round': round_index, 'case': case,
                        'trace': commands, 'layout': best, 'validation': validation})
        finally:
            collector.close()
        if len(rollout['actions']) != 1024 or policy_hash(anchor.state_dict()) != initial_hash:
            raise ValueError('update_stability.collection: command count or anchor changed')
        torch.save(rollout, output/'rollout.pt')
        write(output/'collection.json', collection_evidence)
        result['collection'] = {'commands': 1024, 'episodes': 64, 'rollout_sha256': digest(output/'rollout.pt'),
                                 'replay_sha256': digest(output/'collection.json'), 'seconds': collector.seconds}
        actors = {'anchor': anchor}
        for multiplier in spec['learning_rate_multipliers']:
            active = f'lr{multiplier:g}'
            actor = deepcopy(anchor)
            optimizer = torch.optim.Adam(actor.parameters(), lr=config['learning_rate'])
            optimizer.load_state_dict(deepcopy(saved['optimizer']))
            for group in optimizer.param_groups:
                group['lr'] = config['learning_rate']*multiplier
            branch_started = time.perf_counter()
            metrics = measured_update(actor, optimizer, rollout, config, np.random.default_rng(spec['shuffle_seed']))
            if metrics['optimizer_steps'] != 64:
                raise ValueError('update_stability.accounting: expected 64 optimizer steps per branch')
            if multiplier == 0 and any(not torch.equal(a, b) for a, b in zip(anchor.parameters(), actor.parameters())):
                raise ValueError('update_stability.control: zero-rate branch changed parameters')
            actors[active] = actor
            path = output/f'{active}.pt'
            torch.save({'purpose': 'diagnostic_only', 'state_dict': actor.state_dict()}, path)
            result['branches'][active] = {'multiplier': multiplier, 'metrics': metrics,
                'seconds': time.perf_counter()-branch_started, 'policy_sha256': policy_hash(actor.state_dict()),
                'file_sha256': digest(path)}
            write(output/'stability.json', result)
            print(json.dumps({'condition': active, 'distances': metrics['deployment_distances']}), flush=True)
        hashes = {name: policy_hash(actor.state_dict()) for name, actor in actors.items()}
        for case, path in cases.items():
            runtime, catalog = Runtime(root, path, config, library), Catalog(root, path)
            paired = {}
            for condition, actor in actors.items():
                active = f'{case}/{condition}'
                cell = evaluate(actor, runtime, catalog, config, spec, output, case, condition)
                result['cells'].append(cell)
                paired[condition] = cell['episodes']
                write(output/'stability.json', result)
            result['pairs'].append({'case': case, **{f'{name}_vs_anchor': paired_counts(records, paired['anchor'])
                                                     for name, records in paired.items() if name != 'anchor'}})
            print(json.dumps(result['pairs'][-1]), flush=True)
        if len(result['cells']) != 80 or any(policy_hash(actor.state_dict()) != hashes[name] for name, actor in actors.items()):
            raise ValueError('update_stability.final: incomplete evaluation or frozen actor changed')
        if digest(checkpoint) != result['anchor']['checkpoint_sha256']:
            raise ValueError('update_stability.final: original checkpoint changed')
        result.update(status='complete', total_seconds=time.perf_counter()-started,
                      evaluation_commands=163840, optimizer_steps=256)
    except BaseException as error:
        result.update(status='interrupted', diagnostic={'code': 'update_stability.interrupted', 'severity': 'error',
            'stage': 'single_update_stability', 'message': str(error), 'entity': active})
        write(output/'stability.json', result)
        raise
    write(output/'stability.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--protocol', type=Path, required=True)
    parser.add_argument('--native-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run(args.root.resolve(), args.source.resolve(), args.protocol.resolve(), args.native_library.resolve(), args.output.resolve())
