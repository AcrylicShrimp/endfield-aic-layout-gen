"""Small permutation-equivariant policies; no environment outcomes enter forward()."""

import torch
from torch import nn


class Policy(nn.Module):
    def __init__(self, token_dim, action_dim, width=32, architecture="graph"):
        super().__init__()
        if architecture not in ("graph", "pooled") or width < 2 or width % 2:
            raise ValueError("Expected graph/pooled architecture and positive even width")
        self.config = dict(token_dim=token_dim, action_dim=action_dim, width=width,
                           architecture=architecture)
        self.token = nn.Sequential(nn.Linear(token_dim, width), nn.ReLU())
        self.action = nn.Sequential(nn.Linear(action_dim, width), nn.ReLU())
        self.messages = nn.ModuleList()
        self.norms = nn.ModuleList()
        if architecture == "graph":
            self.messages.extend(nn.Sequential(nn.Linear(width * 3, width), nn.ReLU(),
                                               nn.Linear(width, width)) for _ in range(2))
            self.norms.extend(nn.LayerNorm(width) for _ in range(2))
            self.attention = nn.MultiheadAttention(width, 2, dropout=0, batch_first=True)
        self.head = nn.Sequential(nn.Linear(width * 2, width), nn.ReLU(), nn.Linear(width, 1))

    def forward(self, tokens, adjacency, actions, mask):
        h, q = self.token(tokens), self.action(actions)
        if self.config["architecture"] == "graph":
            incoming = adjacency.transpose(1, 2)
            for message, norm in zip(self.messages, self.norms):
                # Mean aggregation, separately preserving source-to-target direction.
                ins = incoming.bmm(h) / incoming.sum(-1, keepdim=True).clamp_min(1)
                outs = adjacency.bmm(h) / adjacency.sum(-1, keepdim=True).clamp_min(1)
                h = norm(h + message(torch.cat([h, ins, outs], dim=-1)))
                h = h * mask.unsqueeze(-1)
            context, _ = self.attention(q, h, h, key_padding_mask=~mask, need_weights=False)
        else:
            context = ((h * mask.unsqueeze(-1)).sum(1) / mask.sum(1, keepdim=True))
            context = context.unsqueeze(1).expand(-1, q.shape[1], -1)
        return self.head(torch.cat([q, context], dim=-1)).squeeze(-1)


def fit_normalization(samples):
    """Only actual, unpadded training rows contribute statistics."""
    if not samples or any(s["split"] != "train" for s in samples):
        raise ValueError("Normalization accepts training samples only")
    result = {}
    for field in ("tokens", "actions"):
        values = torch.tensor([row for s in samples for row in s["features"][field]],
                              dtype=torch.float32)
        result[field] = {"mean": values.mean(0).tolist(),
                         "scale": values.std(0, correction=0).clamp_min(1).tolist()}
    return result


def pack(samples, normalization, device="cpu"):
    if not samples:
        raise ValueError("Cannot encode an empty sample set")
    count = max(len(s["features"]["tokens"]) for s in samples)
    token_dim = len(normalization["tokens"]["mean"])
    tokens = torch.zeros(len(samples), count, token_dim)
    adjacency = torch.zeros(len(samples), count, count)
    mask = torch.zeros(len(samples), count, dtype=torch.bool)
    actions = torch.tensor([s["features"]["actions"] for s in samples], dtype=torch.float32)
    for i, s in enumerate(samples):
        n = len(s["features"]["tokens"])
        tokens[i, :n] = torch.tensor(s["features"]["tokens"], dtype=torch.float32)
        adjacency[i, :n, :n] = torch.tensor(s["features"]["adjacency"], dtype=torch.float32)
        mask[i, :n] = True
    for field, tensor in (("tokens", tokens), ("actions", actions)):
        tensor.sub_(torch.tensor(normalization[field]["mean"]))
        tensor.div_(torch.tensor(normalization[field]["scale"]))
    tokens *= mask.unsqueeze(-1)
    return tuple(t.to(device) for t in (tokens, adjacency, actions, mask))


def checkpoint(model, normalization):
    return {"schema_version": 1, "architecture": model.config, "normalization": normalization,
            "weights": {k: v.detach().cpu().tolist() for k, v in model.state_dict().items()}}


def restore(value, device="cpu"):
    if value["schema_version"] != 1:
        raise ValueError("Unsupported checkpoint schema")
    model = Policy(**value["architecture"]).to(device)
    model.load_state_dict({k: torch.tensor(v, device=device) for k, v in value["weights"].items()},
                          strict=True)
    if any(not torch.isfinite(v).all() for v in model.state_dict().values()):
        raise ValueError("Nonfinite checkpoint weights")
    return model
