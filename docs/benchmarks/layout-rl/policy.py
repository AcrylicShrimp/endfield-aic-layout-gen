"""Variable-size action-conditioned policy, reusing the previous small graph encoder."""

from pathlib import Path
import importlib.util

import torch

_spec = importlib.util.spec_from_file_location("aic_learning_policy", Path(__file__).resolve().parent.parent / "layout-learning" / "model.py")
_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_module)
Policy = _module.Policy


def pack(observations):
    features = [o["features"] for o in observations]
    if not features:
        raise ValueError("Cannot encode empty observations")
    nt = max(len(f["tokens"]) for f in features)
    na = max(len(f["actions"]) for f in features)
    td, ad = len(features[0]["tokens"][0]), len(features[0]["actions"][0])
    tokens = torch.zeros(len(features), nt, td)
    adjacency = torch.zeros(len(features), nt, nt)
    actions = torch.zeros(len(features), na, ad)
    token_mask = torch.zeros(len(features), nt, dtype=torch.bool)
    action_mask = torch.zeros(len(features), na, dtype=torch.bool)
    for i, f in enumerate(features):
        n, a = len(f["tokens"]), len(f["actions"])
        tokens[i, :n] = torch.tensor(f["tokens"])
        adjacency[i, :n, :n] = torch.tensor(f["adjacency"])
        actions[i, :a] = torch.tensor(f["actions"])
        token_mask[i, :n] = True
        action_mask[i, :a] = torch.tensor(f["action_mask"])
    if not action_mask.any(-1).all() or not all(torch.isfinite(t).all() for t in (tokens, adjacency, actions)):
        raise ValueError("Invalid finite features or empty action domain")
    return (tokens, adjacency, actions, token_mask), action_mask


def distribution(model, observations):
    inputs, action_mask = pack(observations)
    logits = model(*inputs).masked_fill(~action_mask, -torch.inf)
    return torch.distributions.Categorical(logits=logits)


def checkpoint(model):
    return dict(schema_version=1, architecture=model.config,
                weights={k: v.detach().cpu().tolist() for k, v in model.state_dict().items()})


def restore(value):
    if value["schema_version"] != 1:
        raise ValueError("Unsupported model schema")
    model = Policy(**value["architecture"])
    model.load_state_dict({k: torch.tensor(v) for k, v in value["weights"].items()}, strict=True)
    if not all(torch.isfinite(v).all() for v in model.state_dict().values()):
        raise ValueError("Nonfinite model weights")
    return model


def terminal_advantages(tuples):
    """Exact lexicographic comparisons; equal final incumbents receive equal return."""
    if len(tuples) < 2:
        raise ValueError("At least two trajectories are required")
    values = [tuple(t) for t in tuples]
    return torch.tensor([sum((b > a) - (b < a) for b in values) / (len(values) - 1)
                         for a in values], dtype=torch.float32)
