"""One-time authentic tensor export for the Rust numerical/recovery gate.

This is not a training data loader. Recorded successful labels supply immutable
inputs and returns. PPO targets are explicitly frozen for a numerical kernel
comparison; this fixture is not a reconstructed on-policy rollout or quality run.
Requires the same Torch version as the Rust backend and the safetensors package.
"""
import argparse
from collections import Counter
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import torch
from safetensors.torch import save_file

from blueprint_execution_update import prepare_minibatch
from blueprint_ppo_store import checked, load, read, sha, write
from blueprint_ppo_update import actor_advantages, require
from blueprint_success_buffer import Evidence
from declarative_replay_storage import sample_input
import declarative_learning_policy as policy


def state_tensors(model, actor, critic, generators):
    result = {f'model.{name}': value for name, value in model.state_dict().items()}
    for label, optimizer in (('actor', actor), ('critic', critic)):
        owned = {id(p) for group in optimizer.param_groups for p in group['params']}
        for name, parameter in model.named_parameters():
            if id(parameter) not in owned:
                continue
            state = optimizer.state.get(parameter, {})
            step = int(state.get('step', 0))
            prefix = f'adam.{label}.{name}'
            result[f'{prefix}.step'] = torch.tensor([step], dtype=torch.int64)
            if step:
                result[f'{prefix}.mean'] = state['exp_avg']
                result[f'{prefix}.square'] = state['exp_avg_sq']
    result.update({f'rng.{name}': generator.get_state() for name, generator in generators.items()})
    return {name: value.detach().cpu().contiguous().clone() for name, value in result.items()}


def export(source, output, max_labels=64):
    require(not output.exists(), f'rust-learner.export-output: already exists: {output}')
    require(1 <= max_labels <= 256, 'rust-learner.export-bound: labels must be in 1..256')
    torch.set_num_threads(1)
    torch.set_num_interop_threads(1)
    torch.use_deterministic_algorithms(True)
    job = read(source/'state.json')
    launch = read(source.parent/'launch.json')
    config = launch['config']
    checkpoint_path = checked(job['latest_checkpoint'], source)
    checkpoint = load(checkpoint_path)
    model, actor, critic = policy.learners(checkpoint['seed'], config['ppo'], 'cpu')
    model.load_state_dict(checkpoint['model'], strict=True)
    actor.load_state_dict(checkpoint['actor_optimizer'])
    critic.load_state_dict(checkpoint['critic_optimizer'])
    # All modes being imported must be represented by the explicit native Adam.
    for optimizer in (actor, critic):
        for group in optimizer.param_groups:
            require(not any(group.get(k, False) for k in ('amsgrad', 'maximize', 'capturable', 'differentiable', 'fused', 'decoupled_weight_decay'))
                    and group['weight_decay'] == 0, 'rust-learner.adam-mode: unsupported imported configuration')
    generators = {name: torch.Generator() for name in ('sampler', 'shuffle', 'replay', 'replay_order')}
    generators['sampler'].set_state(checkpoint['sampler_rng'])
    generators['shuffle'].set_state(checkpoint['shuffle_rng'])
    generators['replay'].manual_seed(5_000_000+checkpoint['seed']*1000+checkpoint['update']+1)
    generators['replay_order'].manual_seed(6_000_000+checkpoint['seed']*1000+checkpoint['update']+1)
    evidence = Evidence(source, job)
    entries = checkpoint['buffer']['entries']
    # Prefer complete, short sequences with a placement so the test includes
    # conditional port factors; never truncate a successful sequence's credit.
    ordered = sorted(entries.values(), key=lambda e: (
        not any(label['stage'] == 'place' for label in e['labels']),
        len({label['stage'] for label in e['labels']}) < 2, len(e['labels']), e['id']))
    sequences, rows, references = [], [], []
    for entry in ordered:
        if len(rows)+len(entry['labels']) > max_labels:
            continue
        labels = []
        for label in entry['labels']:
            sample = evidence.sample(label['reference'])
            labels.append(dict(input=sample_input(sample), decision=sample['decision'],
                stage=label['stage'], return_to_terminal=label['return_to_terminal'], reference=label['reference']))
            references.append(label['reference'])
        sequences.append(dict(id=entry['id'], rows=labels, origin='retained-authentic', stratum='diagnostic'))
        rows.extend(labels)
        if len(sequences) == 8:
            break
    require(rows and any(row['stage'] == 'place' for row in rows), 'rust-learner.source: need retained placement successes')
    packed = policy.prepare_rows(rows)
    with torch.no_grad():
        old_logp, _, old_values = policy.predict(model, policy.device_rows(packed, 'cpu'))
    returns = torch.tensor([row['return_to_terminal'] for row in rows], dtype=torch.float32)
    advantages = actor_advantages(returns-old_values, config['ppo'].get('advantage_normalization', 'standardize'))
    ppo_rows = [dict(row, old_logp=float(lp)) for row, lp in zip(rows, old_logp, strict=True)]
    prepared = prepare_minibatch(ppo_rows, list(range(len(rows))), advantages, returns, sequences, policy)
    tensors = state_tensors(model, actor, critic, generators)
    for prefix in ('ppo', 'sil'):
        value = prepared[prefix]
        for section in ('batch', 'branches'):
            for name, tensor in (value[section] or {}).items():
                if isinstance(tensor, torch.Tensor):
                    tensors[f'{prefix}.{section}.{name}'] = tensor
        for name in ('roots', 'selected'):
            tensors[f'{prefix}.{name}'] = value[name]
    tensors.update({'ppo.old_logp': prepared['old_logp'], 'ppo.advantages': prepared['advantages'],
        'ppo.targets': prepared['targets'], 'sil.returns': prepared['sil']['returns'], 'sil.weights': prepared['sil']['weights']})
    tensors = {name: value.detach().cpu().contiguous().clone() for name, value in tensors.items()}
    shape = model.state_dict()
    parameters = actor.param_groups[0]
    output.mkdir(parents=True)
    path = output/'input.safetensors'
    save_file(tensors, path)
    manifest = dict(schema='aic-rust-learner-input-v1', tensors=path.name, tensors_sha256=sha(path),
        max_tensor_bytes=256*1024*1024,
        source=dict(directory=str(source.resolve()), checkpoint=str(checkpoint_path.resolve()), checkpoint_sha256=sha(checkpoint_path),
            launch_sha256=sha(source.parent/'launch.json'), source_update=checkpoint['update'],
            export_torch_version=torch.__version__, native_library_sha256=launch['source']['native_sha256'],
            sequence_ids=[s['id'] for s in sequences], references=references,
            stage_counts=dict(Counter(r['stage'] for r in rows)), positive_replay_labels=int((returns-old_values > 0).sum()),
            ppo_target_contract='Frozen recorded-success terminal returns for numerical comparison, not a new on-policy rollout'),
        model=dict(width=config['ppo']['width'], message_rounds=config['ppo']['message_rounds'],
            node_features=shape['nodes.0.weight'].shape[1], edge_features=shape['edges.0.weight'].shape[1],
            global_features=shape['context.0.weight'].shape[1]-config['ppo']['width'],
            action_features=prepared['ppo']['batch']['action_features'].shape[1], port_features=policy.PORT_FEATURES),
        update=dict(learning_rate=parameters['lr'], beta1=parameters['betas'][0], beta2=parameters['betas'][1],
            epsilon=parameters['eps'], clip_epsilon=config['ppo']['clip_epsilon'],
            entropy_coefficient=config['ppo']['entropy_coefficient'], imitation_coefficient=config['success']['coefficient'],
            gradient_clip=config['ppo']['gradient_clip']),
        parameter_order=[name for name, _ in model.named_parameters()], ppo_graphs=len(rows), sil_graphs=len(rows),
        ppo_factors=len(prepared['ppo']['selected']), sil_factors=len(prepared['sil']['selected']))
    require(manifest['source']['positive_replay_labels'] > 0, 'rust-learner.source: no positive imitation advantages in selected actual labels')
    write(output/'manifest.json', manifest)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--max-labels', type=int, default=64)
    args = parser.parse_args()
    value = export(args.source, args.output, args.max_labels)
    print(dict(labels=value['ppo_graphs'], conditional_factors=value['ppo_factors'], source=value['source']))


if __name__ == '__main__':
    main()
