"""Fixed-data PPO component gradients and local parameter-displacement summaries."""

import numpy as np
import torch

from policy import advantages, batch


COMPONENTS = ('policy', 'weighted_value', 'negative_weighted_entropy')


def targets(rollout, config):
    advantage, returns = advantages(rollout['rewards'], rollout['values'], rollout['dones'],
                                    0.0, config['gamma'], config['gae_lambda'])
    return (advantage-advantage.mean())/(advantage.std()+1e-8), returns


def losses(logits, values, selected, old, advantage, returns, config):
    log_probs = logits.log_softmax(-1)
    chosen = log_probs.gather(1, selected[:, None]).squeeze(1)
    ratio = (chosen-old).exp()
    return torch.stack((torch.maximum(-advantage*ratio,
                        -advantage*ratio.clamp(1-config['clip'], 1+config['clip'])).mean(),
                        config['value_coefficient']*(values-returns).square().mean(),
                        config['entropy_coefficient']*(log_probs.exp()*log_probs).sum(-1).mean()))


def inputs(model, rollout, indices, advantage, returns):
    device = next(model.parameters()).device
    data = {k: v.to(device) for k, v in batch([rollout['observations'][i] for i in indices]).items()}
    args = (torch.tensor([rollout['actions'][i] for i in indices], device=device),
            torch.tensor([rollout['log_probs'][i] for i in indices], device=device),
            torch.from_numpy(advantage[indices]).to(device), torch.from_numpy(returns[indices]).to(device))
    return data, args


def components(model, rollout, indices, advantage, returns, config, gradients):
    data, args = inputs(model, rollout, indices, advantage, returns)
    with torch.set_grad_enabled(gradients):
        values = losses(*model(data), *args, config)
        vectors = []
        if gradients:
            parameters = list(model.parameters())
            for i in range(len(COMPONENTS)):
                derivatives = torch.autograd.grad(values[i], parameters, retain_graph=i < 2, allow_unused=True)
                vectors.append(torch.cat([(torch.zeros_like(p) if g is None else g).detach().flatten()
                                          for p, g in zip(parameters, derivatives)]).double().cpu())
    numbers = values.detach().double().cpu()
    if not torch.isfinite(numbers).all() or any(not torch.isfinite(g).all() for g in vectors):
        raise ValueError('gradient_contributions.nonfinite: component objective or derivative')
    return numbers, torch.stack(vectors) if gradients else None


def cosine(a, b):
    denominator = torch.linalg.vector_norm(a)*torch.linalg.vector_norm(b)
    return float(torch.dot(a, b)/denominator) if denominator > 0 else None


def summarize(gradients, delta):
    """Input shape: equal-sized cases by loss components by parameter coordinates."""
    require_shape = gradients.ndim == 3 and gradients.shape[0] > 1 and gradients.shape[1] == 3
    if (not require_shape or delta.shape != gradients.shape[2:]
            or not torch.isfinite(gradients).all() or not torch.isfinite(delta).all()):
        raise ValueError('gradient_contributions.shape: finite cases/components/parameters and matching displacement required')
    pooled = gradients.mean(0)
    total = gradients.sum(1)
    rows = []
    for index, gradient in enumerate(gradients):
        other_policy = (gradients[:, 0].sum(0)-gradient[0])/(len(gradients)-1)
        rows.append({'component_norms': dict(zip(COMPONENTS, gradient.norm(dim=1).tolist())),
                     'component_dot_displacement': dict(zip(COMPONENTS, (gradient@delta).tolist())),
                     'total_norm': float(total[index].norm()),
                     'total_dot_displacement': float(total[index]@delta),
                     'policy_cosine_other_cases': cosine(gradient[0], other_policy)})
    return {'cases': rows, 'parameter_displacement_norm': float(delta.norm()),
            'pooled_component_norms': dict(zip(COMPONENTS, pooled.norm(dim=1).tolist())),
            'pooled_component_cosines': [[cosine(a, b) for b in pooled] for a in pooled],
            'pairwise_policy_cosines': [[cosine(a, b) for b in gradients[:, 0]] for a in gradients[:, 0]],
            'pairwise_total_cosines': [[cosine(a, b) for b in total] for a in total]}
