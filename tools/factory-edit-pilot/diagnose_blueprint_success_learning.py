"""Bounded saved-policy and replay-attribution probe of selected training successes.

Run from the frozen experiment root. No optimizer, native action or evaluation
factory is used. Historical mixed minibatches remain explicitly unattributed.
"""
import argparse
from collections import Counter
from pathlib import Path
import sys

sys.path.insert(0, str(Path.cwd() / 'tools/factory-edit-pilot'))
import torch

from blueprint_edit_input import BlueprintActorCritic, prepare
from blueprint_ppo_batch import distribution, pack
from blueprint_ppo_store import checked, load, read, reference, sha, write
from blueprint_ppo_update import credit, digest, predictions, require


def gradient_probe(model, samples, frontiers, config, rows, cases, population):
    """Expected rare-entry SIL influence at one saved pre-update model; no step."""
    parameters = [p for name, p in model.named_parameters() if not name.startswith('critic.')]

    def vector(loss):
        parts = torch.autograd.grad(loss, parameters, retain_graph=True, allow_unused=True)
        return torch.cat([(torch.zeros_like(p) if g is None else g).flatten()
                          for p, g in zip(parameters, parts, strict=True)])

    def comparison(vector, baseline):
        norm, other = float(vector.norm()), float(baseline.norm())
        return dict(norm=norm, norm_ratio=norm / other if other else None,
                    cosine=float(torch.dot(vector, baseline)) / (norm * other) if norm and other else None)

    _, advantages, targets = credit(samples, frontiers, config['ppo'])
    ppo = torch.zeros(sum(p.numel() for p in parameters))
    exploration = torch.zeros_like(ppo)
    shared_value = torch.zeros_like(ppo)
    for start in range(0, len(samples), config['ppo']['minibatch_size']):
        batch_rows = samples[start:start + config['ppo']['minibatch_size']]
        logp, entropy, values = predictions(model, batch_rows)
        torch.testing.assert_close(logp, torch.tensor([s['old_logp'] for s in batch_rows]), rtol=1e-5, atol=2e-6)
        torch.testing.assert_close(values, torch.tensor([s['old_value'] for s in batch_rows]), rtol=1e-5, atol=2e-6)
        ratio = (logp - torch.tensor([s['old_logp'] for s in batch_rows])).exp()
        clipped = ratio.clamp(1 - config['ppo']['clip_epsilon'], 1 + config['ppo']['clip_epsilon'])
        advantage = advantages[start:start + len(batch_rows)]
        policy_loss = -torch.minimum(ratio * advantage, clipped * advantage).sum() / len(samples)
        ppo += vector(policy_loss)
        exploration += vector(-config['ppo']['entropy_coefficient'] * entropy.sum() / len(samples))
        shared_value += vector(.5 * (values - targets[start:start + len(batch_rows)]).square().sum() / len(samples))
    actor = ppo + exploration + shared_value
    measured, total, by_stage = [], torch.zeros_like(actor), {}
    for case in cases:
        labels = case['labels']
        logp, _, values = predictions(model, [rows[label['row']] for label in labels])
        returns = torch.tensor([label['return_to_terminal'] for label in labels])
        weights = (returns - values.detach()).clamp_min(0)
        stages = sorted({label['stage'] for label in labels})
        components = []
        for stage in stages:
            indices = [i for i, label in enumerate(labels) if label['stage'] == stage]
            loss = (-logp[indices] * weights[indices]).mean()
            # Uniform sequence sampling with replacement, then stage averaging.
            g = vector(loss * config['success']['coefficient'] / population / len(stages))
            total += g
            by_stage[stage] = by_stage.get(stage, torch.zeros_like(actor)) + g
            components.append(dict(stage=stage, weights=weights[indices].tolist(),
                                   expected_gradient_vs_ppo_actor=comparison(g, actor)))
        measured.append(dict(id=case['id'], stages=components))
    return dict(buffer_population=population, ppo_policy_gradient_norm=float(ppo.norm()),
        entropy_gradient_vs_ppo_policy=comparison(exploration, ppo),
        shared_value_gradient_vs_ppo_policy=comparison(shared_value, ppo),
        ppo_actor_gradient_norm=float(actor.norm()), cases=measured,
        rare_expected_gradient_vs_ppo_actor=comparison(total, actor),
        stages={stage: comparison(g, actor) for stage, g in by_stage.items()},
        stage_pair_cosines={f'{a}/{b}': comparison(by_stage[a], by_stage[b])['cosine']
                            for a in by_stage for b in by_stage if a < b})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--matrix', type=Path, required=True)
    parser.add_argument('--selection', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(not args.output.exists(), f'fresh diagnostic output required: {args.output}')
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    matrix = args.matrix.resolve()
    protocol = read(matrix / 'matrix.json')
    selected = read(args.selection)
    require(len(selected) == 5 and len({s['entry_id'] for s in selected}) == 5,
            f'expected five distinct retained successes: {args.selection}')
    require(read(matrix / 'state.json')['status'] == 'complete', f'completed matrix required: {matrix}')
    sources, cases, rows, history, probes, gradients = {}, [], [], [], [], []

    def bound(ref, root):
        path = (root / ref['path']).resolve()
        if str(path) not in sources:
            checked(ref, root)
            sources[str(path)] = ref['sha256']
        require(sources[str(path)] == ref['sha256'], f'conflicting evidence identity: {path}')
        return path

    for name in sorted({s['arm'] for s in selected}):
        arm = next(a for a in protocol['arms'] if a['name'] == name)
        root, seed = matrix / arm['directory'], arm['seed']
        arm_protocol = read(bound(arm['protocol'], matrix))
        config = arm_protocol['config']
        for source_name in ('blueprint_edit_input.py', 'blueprint_ppo_batch.py', 'blueprint_ppo_update.py',
                     'blueprint_success_update.py', 'blueprint_success_buffer.py'):
            path = f'tools/factory-edit-pilot/{source_name}'
            bound(dict(path=path, sha256=arm_protocol['sources'][path]), Path.cwd())
        job = read(root / 'state.json')['jobs'][str(seed)]
        final = load(bound(job['latest_checkpoint'], root))
        records = [read(bound(ref, root)) for ref in job['records']]
        chunks = {}
        for choice in (s for s in selected if s['arm'] == name):
            entry = final['buffer']['entries'][choice['entry_id']]
            require(entry['segment']['episode'] * 8 + entry['segment']['stream'] == choice['slot'],
                    f'selected training identity: {choice["entry_id"]}')
            labels = []
            for label in entry['labels']:
                ordinal, index = label['reference']
                if ordinal not in chunks:
                    chunk = load(bound(records[ordinal - 1]['chunk'], root))
                    raw, normalized, targets = credit(chunk['samples'], chunk['frontiers'], config['ppo'])
                    require(digest(normalized) == chunk['ppo']['advantages_sha256']
                            and digest(targets) == chunk['ppo']['targets_sha256'], 'source PPO credit identity')
                    chunks[ordinal] = (chunk, raw, normalized)
                chunk, raw, normalized = chunks[ordinal]
                sample = chunk['samples'][index]
                require(digest(prepare(sample['observation'])) == digest(sample['input']),
                        f'recorded input agrees with frozen preparation: {label["reference"]}')
                require(label['stage'] != 'commit' or sample['validated_witness'] is not None,
                        'selected success has a native validated witness')
                row_id = len(rows)
                rows.append(dict(input=sample['input'], index=sample['index']))
                labels.append(dict(row=row_id, **label, policy_action=sample['policy_action'],
                    action=sample['input']['actions'][sample['index']], behavior_probability=float(torch.tensor(sample['old_logp']).exp()),
                    behavior_value=sample['old_value'], source_ppo_raw=float(raw[index]),
                    source_ppo_actor_advantage=float(normalized[index])))
            cases.append(dict(arm=name, slot=choice['slot'], product=choice['product'], id=entry['id'],
                admitted_update=entry['admitted_update'], uses=entry['uses'], labels=labels))
            counts = Counter()
            for record in records:
                success = record['ppo']['success']
                groups, diagnostics = success['selected_ids'], success.get('diagnostics', [])
                require(len(groups) == len(diagnostics), 'aligned replay selections and diagnostics')
                for group, diagnostic in zip(groups, diagnostics, strict=True):
                    count = group.count(entry['id'])
                    if not count:
                        continue
                    kind = ('all_positive' if diagnostic['positive'] == diagnostic['labels'] else
                            'all_zero' if diagnostic['positive'] == 0 else 'mixed_unattributed')
                    counts[kind] += count
            require(sum(counts.values()) == entry['uses'], f'historical replay-use identity: {entry["id"]}')
            history.append(dict(id=entry['id'], **dict(counts)))
        print(dict(stage='source_attribution', arm=name, selected_cases=len(cases)), flush=True)

    batch = pack([r['input'] for r in rows])
    for arm in protocol['arms']:
        root, seed = matrix / arm['directory'], arm['seed']
        config = read(bound(arm['protocol'], matrix))['config']
        job = read(root / 'state.json')['jobs'][str(seed)]
        checkpoints = {0, 128, 256, 384, 512}
        checkpoints.update(((c['admitted_update'] + 3) // 4) * 4 for c in cases if c['arm'] == arm['name'])
        for ordinal in sorted(checkpoints):
            if ordinal == 0:
                ref = reference(root / f'seed-{seed}/checkpoints/0000.pt.gz', root)
            else:
                record = read(bound(job['records'][ordinal - 1], root))
                ref = record['checkpoint']
            saved = load(bound(ref, root))['learner']
            require(saved['update'] == ordinal and saved['protocol_sha256'] == arm['protocol']['sha256'],
                    'checkpoint update/protocol identity')
            require(digest(saved['model']) == saved['model_sha256'], 'saved model identity')
            model = BlueprintActorCritic(config['ppo']['width'], config['ppo']['message_rounds'])
            model.load_state_dict(saved['model'], strict=True)
            model.eval()
            with torch.no_grad():
                logits, values = model.forward_packed(batch)
                logp, _ = distribution(logits, batch)
            offset, predictions = 0, []
            for i, row in enumerate(rows):
                actions = row['input']['actions']
                probabilities = logp[offset:offset + len(actions)].exp()
                require(abs(float(probabilities.sum()) - 1) < 1e-5, 'normalized candidate distribution')
                selected_probability = float(probabilities[row['index']])
                kinds = {kind: sum(float(p) for p, action in zip(probabilities, actions, strict=True)
                                   if action['kind'] == kind) for kind in sorted({a['kind'] for a in actions})}
                label = next(label for c in cases for label in c['labels'] if label['row'] == i)
                weight = max(label['return_to_terminal'] - float(values[i]), 0.)
                predictions.append(dict(row=i, probability=selected_probability,
                    rank=1 + int((probabilities > probabilities[row['index']]).sum()), candidates=len(actions),
                    top_action=actions[int(probabilities.argmax())], kind_probabilities=kinds,
                    conditional_kind_probability=selected_probability / kinds[actions[row['index']]['kind']],
                    value=float(values[i]), positive_sil_weight=weight,
                    coefficient_weighted_nll=-config['success']['coefficient'] * weight * float(logp[offset + row['index']])))
                offset += len(actions)
            probes.append(dict(arm=arm['name'], update=ordinal, model_sha256=saved['model_sha256'], rows=predictions))
        print(dict(stage='saved_policy_inference', arm=arm['name'], checkpoints=sorted(checkpoints)), flush=True)
        own_cases = [c for c in cases if c['arm'] == arm['name']]
        if own_cases:
            before_record = read(bound(job['records'][510], root))
            before = load(bound(before_record['checkpoint'], root))
            require(before['learner']['update'] == 511, 'gradient probe starts at update 511 checkpoint')
            model.load_state_dict(before['learner']['model'], strict=True)
            record = read(bound(job['records'][511], root))
            chunk = load(bound(record['chunk'], root))
            require(digest(model.state_dict()) == chunk['ppo']['model_before'], 'gradient model/source chunk identity')
            gradients.append(dict(arm=arm['name'], update=512, **gradient_probe(model,
                chunk['samples'], chunk['frontiers'], config, rows, own_cases, len(before['buffer']['entries']))))
            print(dict(stage='backward_only_probe', arm=arm['name'], update=512), flush=True)
    write(args.output, dict(schema='blueprint-success-learning-diagnostic-v1',
        scope='recorded training states; no new training or native action; mixed history is unattributed',
        matrix_sha256=sha(matrix / 'matrix.json'), selection_sha256=sha(args.selection),
        script_sha256=sha(Path(__file__)), sources=sources, cases=cases, history=history, probes=probes,
        gradient_scope='pre-clipping expected rare SIL versus full-batch PPO actor; no optimizer step', gradients=gradients))
    print(dict(status='complete', cases=len(cases), labels=len(rows), checkpoints=len(probes), output=str(args.output)), flush=True)


if __name__ == '__main__':
    main()
