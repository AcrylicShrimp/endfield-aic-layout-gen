"""Finish a bounded critic fit while preserving policy parameters and Adam."""
import time

import torch

from full_factory_on_policy import parameter_hash
from ppo_actor_critic import optimizer_parameters, require
from ppo_training_batch import critic_values
from ppo_training_supply import prepared_batches
from ppo_training_transfer import move_prepared
from random_factory_audit import optimizer_hash


def finish(actor, critic, actor_optimizer, critic_optimizer, shuffle_rng, steps, advantages,
           targets, config, completed_epochs, microbatch, device, guard, executor, statistics):
    before_actor = parameter_hash(actor)
    before_optimizer = optimizer_hash(actor_optimizer)
    before_rng = shuffle_rng.get_state().clone()
    tail_rng = torch.Generator().set_state(before_rng)
    parameters = list(actor.parameters())
    flags = [p.requires_grad for p in parameters]
    critic_parameters = optimizer_parameters(critic_optimizer)
    rows, optimizer_steps = [], 0
    began = time.perf_counter()
    try:
        for parameter in parameters:
            parameter.requires_grad_(False)
            parameter.grad = None
        for epoch in range(completed_epochs, config['epochs']):
            order = torch.randperm(len(steps), generator=tail_rng).tolist()
            norms = []
            epoch_began = time.perf_counter()
            for _, chunks, _, _ in prepared_batches(executor, steps, advantages, targets, config,
                    actor.width, microbatch, order, config['minibatch_size'], guard):
                guard()
                critic_optimizer.zero_grad(set_to_none=True)
                count = sum(chunk['batch']['size'] for chunk in chunks)
                for chunk in chunks:
                    batch = move_prepared(chunk['batch'], device)
                    target = chunk['targets'].to(device)
                    (.5 * (critic_values(critic, batch) - target).square().sum() / count).backward()
                norm = torch.nn.utils.clip_grad_norm_(critic_parameters, config['gradient_clip'])
                require(bool(torch.isfinite(norm)), 'finite critic-only gradient')
                critic_optimizer.step()
                norms.append(float(norm))
                optimizer_steps += 1
            metrics, _ = statistics(actor, critic, steps, advantages, targets, config,
                                    microbatch, device, guard, executor)
            rows.append(dict(epoch=epoch + 1, critic_optimizer_steps=optimizer_steps,
                             critic_gradient_norm_max=max(norms), seconds=time.perf_counter() - epoch_began, **metrics))
    finally:
        for parameter, flag in zip(parameters, flags):
            parameter.requires_grad_(flag)
    require(parameter_hash(actor) == before_actor and optimizer_hash(actor_optimizer) == before_optimizer
            and torch.equal(shuffle_rng.get_state(), before_rng),
            'critic-only tail must preserve actor, actor Adam and joint shuffle stream')
    return dict(epochs=rows, optimizer_steps=optimizer_steps, seconds=time.perf_counter() - began,
                actor_parameters_and_optimizer_unchanged=True, joint_shuffle_unchanged=True)
