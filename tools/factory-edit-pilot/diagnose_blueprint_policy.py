"""Read-only evidence and loss ablations for a running blueprint duration policy."""
import argparse
from collections import Counter
from copy import deepcopy
from contextlib import ExitStack
from datetime import datetime, timezone
from pathlib import Path
from unittest.mock import patch

import torch

import blueprint_ppo_update as ppo_update
import blueprint_success_update as success_update
from blueprint_edit_input import KINDS, prepare
from blueprint_ppo import check_sources
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_store import checked, load, read, reference, restore_learners, sha, write
from blueprint_ppo_update import credit, digest, predictions, require
from blueprint_success import restore_buffer
from blueprint_success_buffer import Evidence, SuccessBuffer


class Inputs:
    def __init__(self, root):
        self.root = root
        self.references = {}

    def path(self, ref):
        path = checked(ref, self.root)
        self.references[ref['path']] = dict(ref)
        return path

    def json(self, ref):
        return read(self.path(ref))

    def checkpoint(self, update, seed):
        path = self.root / f'seed-{seed}/checkpoints/{update:04d}.pt.gz'
        return restore_buffer(load(self.path(reference(path, self.root))))


def fresh_learners(saved, config, protocol_hash):
    # Adam load_state_dict can retain the supplied state tensors. Each diagnostic
    # arm owns a deep copy so stepping one arm cannot alter another arm's input.
    return restore_learners(deepcopy(saved), config, protocol_hash)


def masses(model, probes, differentiable=False):
    batch = pack(probes)
    logits, values = model.forward_packed(batch)
    logp, entropy = distribution(logits, batch)
    kinds = [a['kind'] for p in probes for a in p['actions']]
    result = {}
    for kind in KINDS:
        selected = torch.tensor([k == kind for k in kinds])
        result[kind] = logp.exp()[selected].sum() / len(probes)
    if differentiable:
        return result['pose']
    return dict(probabilities={k: float(v.detach()) for k, v in result.items()},
                entropy=float(entropy.detach().mean()), value=float(values.detach().mean()))


def measure(model, probes):
    with torch.no_grad():
        return {name: masses(model, rows) for name, rows in probes.items()}


def credit_summary(chunk, config):
    raw, advantages, targets = credit(chunk['samples'], chunk['frontiers'], config)
    groups = {}
    for kind in KINDS:
        indices = [i for i, s in enumerate(chunk['samples']) if s['input']['actions'][s['index']]['kind'] == kind]
        if indices:
            groups[kind] = dict(count=len(indices), raw_sum=float(raw[indices].sum()),
                normalized_sum=float(advantages[indices].sum()),
                positive_credit=int((advantages[indices] > 0).sum()),
                reward=sum(chunk['samples'][i]['reward'] for i in indices))
    outcomes = {}
    for kind, status in sorted({(s['input']['actions'][s['index']]['kind'], s['outcome']['status']) for s in chunk['samples']}):
        indices = [i for i, s in enumerate(chunk['samples'])
                   if s['input']['actions'][s['index']]['kind'] == kind and s['outcome']['status'] == status]
        outcomes[f'{kind}/{status}'] = dict(count=len(indices), raw_sum=float(raw[indices].sum()),
            normalized_sum=float(advantages[indices].sum()), negative_credit=int((advantages[indices] < 0).sum()),
            negative_rewards=sum(chunk['samples'][i]['reward'] < 0 for i in indices),
            reward=sum(chunk['samples'][i]['reward'] for i in indices))
    return dict(by_kind=groups, by_outcome=outcomes, raw_mean=float(raw.mean()), raw_std=float(raw.std(unbiased=False)),
        target_mean=float(targets.mean()), value_mean=sum(s['old_value'] for s in chunk['samples']) / len(chunk['samples']),
        entropy_mean=sum(s['entropy'] for s in chunk['samples']) / len(chunk['samples']),
        zero_reward=all(s['reward'] == 0 for s in chunk['samples']),
        terminal=all(f['terminal'] for f in chunk['frontiers']))


def gradient_components(saved, config, protocol_hash, chunk, sequences, probes, ordinal):
    model, _, _, _, shuffle = fresh_learners(saved, config['ppo'], protocol_hash)
    parameters = [p for name, p in model.named_parameters() if not name.startswith('critic.')]
    objective = masses(model, probes['training'], differentiable=True)
    flatten = lambda gradients: torch.cat([(torch.zeros_like(p) if g is None else g).reshape(-1)
                                           for p, g in zip(parameters, gradients, strict=True)])
    objective_grad = flatten(torch.autograd.grad(objective, parameters, allow_unused=True))
    _, advantages, targets = credit(chunk['samples'], chunk['frontiers'], config['ppo'])
    indices = torch.randperm(len(chunk['samples']), generator=shuffle)[:config['ppo']['minibatch_size']]
    rows = [chunk['samples'][i] for i in indices]
    logp, entropy, values = predictions(model, rows)
    ratio = (logp - torch.tensor([s['old_logp'] for s in rows])).exp()
    clipped = ratio.clamp(1 - config['ppo']['clip_epsilon'], 1 + config['ppo']['clip_epsilon'])
    losses = dict(ppo=-torch.minimum(ratio * advantages[indices], clipped * advantages[indices]).mean(),
        entropy=-config['ppo']['entropy_coefficient'] * entropy.mean(),
        value=.5 * (values - targets[indices]).square().mean())
    if sequences:
        generator = torch.Generator().manual_seed(5_000_000 + saved['seed'] * 1000 + ordinal)
        choices = torch.randint(len(sequences), (config['success']['sequences_per_minibatch'],), generator=generator)
        teacher, _ = success_update.imitation_loss(model, [sequences[i] for i in choices])
        losses['imitation'] = config['success']['coefficient'] * teacher
    result = {}
    for name, loss in losses.items():
        gradient = flatten(torch.autograd.grad(loss, parameters, allow_unused=True, retain_graph=True))
        norm = float(gradient.norm())
        alignment = float(-gradient.dot(objective_grad))
        require(torch.isfinite(gradient).all(), f'nonfinite {name} gradient at update {ordinal}')
        result[name] = dict(loss=float(loss.detach()), actor_gradient_norm=norm,
            pose_probability_derivative_under_gradient_descent=alignment)
    return result


def counterfactual(saved, config, protocol_hash, chunk, sequences, probes, variant):
    frozen_hash = digest(saved)
    model, actor, critic, _, shuffle = fresh_learners(saved, config['ppo'], protocol_hash)
    ppo, success = dict(config['ppo']), dict(config['success'])
    if variant == 'no_imitation':
        success['coefficient'] = 0.
    if variant == 'no_entropy':
        ppo['entropy_coefficient'] = 0.
    with ExitStack() as stack:
        if variant == 'no_value_encoder':
            hook = model.critic.register_forward_pre_hook(lambda module, args: (args[0].detach(),))
            stack.callback(hook.remove)
        if variant in ('no_surrogate', 'centered_unscaled'):
            def transformed(samples, frontiers, settings):
                raw, _, targets = credit(samples, frontiers, settings)
                advantages = torch.zeros_like(raw) if variant == 'no_surrogate' else raw - raw.mean()
                return raw, advantages, targets
            stack.enter_context(patch.object(ppo_update, 'credit', transformed))
            stack.enter_context(patch.object(success_update, 'credit', transformed))
        result = success_update.update_success(model, actor, critic, shuffle, chunk['samples'], chunk['frontiers'],
            ppo, success, sequences, saved['seed'], saved['update'] + 1)
    require(digest(saved) == frozen_hash, f'counterfactual mutated its frozen checkpoint: {variant}')
    return dict(variant=variant, after=measure(model, probes), model_sha256=result['model_after'],
        actor_optimizer_sha256=result['actor_optimizer'], critic_optimizer_sha256=result['critic_optimizer'],
        shuffle_sha256=result['shuffle_rng'], actor_epochs=result['actor_epochs'])


def analyze(root, output, through, updates):
    require(not output.exists(), f'output already exists: {output}')
    inputs = Inputs(root)
    protocol = inputs.json(reference(root / 'protocol.json', root))
    require(protocol['schema'] == 'blueprint-success-duration-v2', f'unsupported protocol: {root}')
    check_sources(protocol)
    state = read(root / 'state.json')
    require(state['protocol_sha256'] == sha(root / 'protocol.json'), 'live protocol binding')
    seed = protocol['config']['seeds'][0]
    job = state['jobs'][str(seed)]
    cutoff = through or job['evaluations'][-1]['update']
    require(cutoff >= max(updates) and cutoff <= job['update'], f'unavailable analysis cutoff {cutoff}')
    output.mkdir(parents=True)
    write(output / 'source-state.json', state)
    config = protocol['config']
    records = [inputs.json(ref) for ref in job['records'][:cutoff]]
    evals = []
    evaluation_zero = None
    for ref in job['evaluations']:
        if ref['update'] > cutoff:
            continue
        report = inputs.json(ref)
        counts = Counter()
        for row in report['rows']:
            counts.update({k: v for k, v in row['counts'].items() if k.startswith(('kind/', 'status/'))})
        evals.append(dict(update=ref['update'], seconds=ref['seconds'], cases=len(report['rows']),
            area_percent=sum(r['total_area_reduction_percent'] for r in report['rows']) / len(report['rows']),
            area_wins=sum(r['best_score'][0] < r['start_score'][0] for r in report['rows']),
            actions=sum(r['policy_actions'] for r in report['rows']), counts=dict(counts)))
        if ref['update'] == 0:
            evaluation_zero = report
    first = load(inputs.path(records[0]['chunk']))
    training_probes = {}
    for row in first['samples']:
        training_probes.setdefault(row['start'], row['input'])
    evaluation_probes = []
    for ref in evaluation_zero['cases']:
        case = load(inputs.path(ref))
        evaluation_probes.append(prepare(case['trace'][0]['observation']))
    probes = dict(training=list(training_probes.values()), evaluation=evaluation_probes)
    probe_hash = digest(probes)
    probe_counts = {group: [dict(Counter(a['kind'] for a in row['actions'])) for row in rows]
                    for group, rows in probes.items()}
    checkpoints = sorted({0, 1, 2, 3, 4, 8, 12, 16, 24, 32, 48, 64, 96, 128, cutoff,
                          *(u - 1 for u in updates)})
    frozen = {}
    policy = []
    for ordinal in checkpoints:
        saved, _ = inputs.checkpoint(ordinal, seed)
        require(saved['update'] == ordinal, f'checkpoint update {ordinal}')
        if ordinal:
            require(saved['model_sha256'] == records[ordinal - 1]['ppo']['model_after'], f'checkpoint model {ordinal}')
        frozen[ordinal] = saved
        model, _, _, _, _ = restore_learners(saved, config['ppo'], state['protocol_sha256'])
        policy.append(dict(update=ordinal, **measure(model, probes)))
    print(dict(stage='fixed-probes', cutoff=cutoff, checkpoints=len(policy)), flush=True)
    credit_rows = []
    chunks = {}
    for record in records:
        ordinal = record['update']
        chunk = load(inputs.path(record['chunk']))
        require(chunk['protocol_sha256'] == state['protocol_sha256'], f'chunk protocol {ordinal}')
        row = credit_summary(chunk, config['ppo'])
        require(abs(row['raw_std'] - record['ppo']['raw_advantage_std']) < 1e-10, f'recorded GAE agreement {ordinal}')
        row.update(update=ordinal, reward=record['area_reward'], model_changed=record['ppo']['model_before'] != record['ppo']['model_after'],
            actor_epochs=record['ppo']['actor_epochs'], buffer_entries=record['buffer']['entries'],
            replay_labels=record['ppo']['success']['positive_labels'])
        credit_rows.append(row)
        if ordinal in updates:
            chunks[ordinal] = chunk
    print(dict(stage='credit', updates=len(credit_rows)), flush=True)
    ablations, gradients, sequences_summary = [], [], []
    evidence = Evidence(root, dict(job, records=job['records'][:cutoff]))
    variants = ('reference', 'no_imitation', 'no_value_encoder', 'no_surrogate', 'no_entropy', 'centered_unscaled')
    for ordinal in updates:
        chunk, saved = chunks[ordinal], frozen[ordinal - 1]
        require(digest(saved['model']) == chunk['model_before'], f'counterfactual behavior {ordinal}')
        buffer = SuccessBuffer(chunk['buffer']['before'])
        sequences = buffer.sequences(evidence, ordinal)
        for entry in buffer.state['entries'].values():
            for label in entry['labels']:
                inputs.path(records[label['reference'][0] - 1]['chunk'])
        sequences_summary.append(dict(update=ordinal, sequences=len(sequences),
            stages=dict(Counter(r['stage'] for s in sequences for r in s['rows'])),
            returns=[r['return_to_terminal'] for s in sequences for r in s['rows']]))
        gradients.append(dict(update=ordinal, components=gradient_components(saved, config, state['protocol_sha256'],
            chunk, sequences, probes, ordinal)))
        results = []
        for variant in variants:
            result = counterfactual(saved, config, state['protocol_sha256'], chunk, sequences, probes, variant)
            if variant == 'reference':
                expected = records[ordinal - 1]['ppo']
                for key, original in [('model_sha256', 'model_after'), ('actor_optimizer_sha256', 'actor_optimizer'),
                                      ('critic_optimizer_sha256', 'critic_optimizer'), ('shuffle_sha256', 'shuffle_rng')]:
                    require(result[key] == expected[original], f'exact reference replay {ordinal}: {key}')
                result['exact_reference_verified'] = True
            if variant == 'no_imitation' and not sequences:
                require(result['model_sha256'] == results[0]['model_sha256']
                        and result['actor_optimizer_sha256'] == results[0]['actor_optimizer_sha256']
                        and result['critic_optimizer_sha256'] == results[0]['critic_optimizer_sha256'],
                        f'empty-buffer counterfactual parity at update {ordinal}')
            results.append(result)
        ablations.append(dict(update=ordinal, before=next(p for p in policy if p['update'] == ordinal - 1), results=results))
        print(dict(stage='ablations', update=ordinal, exact_reference=True), flush=True)
    require(digest(probes) == probe_hash, 'fixed probe mutation')
    check_sources(protocol)
    result = dict(schema='blueprint-policy-diagnosis-v1', status='passed', cutoff=cutoff,
        created_at=datetime.now(timezone.utc).isoformat(), protocol_sha256=state['protocol_sha256'],
        probes=dict(sha256=probe_hash, counts=probe_counts), evaluations=evals, policy=policy,
        credit=credit_rows, gradients=gradients, ablations=ablations, replay_sequences=sequences_summary,
        limitations=['Single seed and recorded trajectories.', 'Ablations are single updates, not new on-policy runs.',
                     'Gradient alignment is Euclidean first-order, not an Adam update prediction.'])
    write(output / 'summary.json', result)
    write(output / 'inputs.json', dict(schema='blueprint-policy-diagnosis-inputs-v1', root=str(root), cutoff=cutoff,
        files=list(inputs.references.values()), source=reference(Path(__file__), Path(__file__).resolve().parents[2])))
    print(dict(status='passed', output=str(output)), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--through', type=int)
    parser.add_argument('--updates', type=int, nargs='+', default=[1, 2, 4, 9, 41, 97, 129])
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    analyze(args.run.resolve(), args.output.resolve(), args.through, args.updates)


if __name__ == '__main__':
    main()
