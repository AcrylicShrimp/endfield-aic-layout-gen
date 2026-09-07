"""CPU CNN plus per-object MLP and a shared explicit-command scorer."""

import numpy as np
import torch
from torch import nn
import time


def batch(observations):
    count = len(observations)
    channels = observations[0]["raster"].shape[0]
    height = max(o["raster"].shape[1] for o in observations)
    width = max(o["raster"].shape[2] for o in observations)
    candidates = max(len(o["candidates"]) for o in observations)
    features = observations[0]["candidates"].shape[1]
    rasters = np.zeros((count, channels, height, width), dtype=np.float32)
    actions = np.zeros((count, candidates, features), dtype=np.float32)
    points = np.zeros((count, candidates, 4), dtype=np.int64)
    mask = np.zeros((count, candidates), dtype=bool)
    for i, o in enumerate(observations):
        _, h, w = o["raster"].shape
        n = len(o["candidates"])
        rasters[i, :, :h, :w] = o["raster"]
        actions[i, :n] = o["candidates"]
        points[i, :n] = o["points"]
        mask[i, :n] = True
    return {"raster": torch.from_numpy(rasters), "candidates": torch.from_numpy(actions),
            "points": torch.from_numpy(points), "mask": torch.from_numpy(mask),
            "objects": torch.from_numpy(np.stack([o["objects"] for o in observations])),
            "stats": torch.from_numpy(np.stack([o["stats"] for o in observations]))}


class Policy(nn.Module):
    def __init__(self, example):
        super().__init__()
        self.dimensions = {"raster_channels": example["raster"].shape[0],
                           "object_rows": example["objects"].shape[0],
                           "object_features": example["objects"].shape[1],
                           "candidate_features": example["candidates"].shape[1],
                           "stats": len(example["stats"])}
        d = self.dimensions
        self.convolutions = nn.ModuleList([nn.Conv2d(d["raster_channels"], 16, 3, padding=1),
                                          nn.Conv2d(16, 32, 3, padding=1),
                                          nn.Conv2d(32, 32, 3, padding=1)])
        self.objects = nn.Sequential(nn.Linear(d["object_features"], 32), nn.ReLU(), nn.Linear(32, 32), nn.ReLU())
        self.fusion = nn.Sequential(nn.Linear(32+d["object_rows"]*32+d["stats"], 64), nn.ReLU(),
                                    nn.Linear(64, 64), nn.ReLU())
        self.scorer = nn.Sequential(nn.Linear(64+64+d["candidate_features"], 64), nn.ReLU(),
                                   nn.Linear(64, 64), nn.ReLU(), nn.Linear(64, 1))
        self.value = nn.Sequential(nn.Linear(64, 64), nn.ReLU(), nn.Linear(64, 1))
        nn.init.orthogonal_(self.scorer[-1].weight, gain=0.01)
        nn.init.zeros_(self.scorer[-1].bias)

    def forward(self, data):
        raster = data["raster"]
        valid = raster[:, -1:]
        spatial = raster
        for layer in self.convolutions:
            # Mask each layer so minibatch padding cannot change an example's logits.
            spatial = torch.relu(layer(spatial))*valid
        pooled = spatial.sum(dim=(2, 3))/valid.sum(dim=(2, 3)).clamp_min(1)
        objects = self.objects(data["objects"]).flatten(1)
        context = self.fusion(torch.cat([pooled, objects, data["stats"]], dim=-1))
        points = data["points"]
        n, c, h, w = spatial.shape
        flat = spatial.flatten(2)
        selected = []
        for offset in (0, 2):
            x = points[:, :, offset].clamp(0, w-1)
            y = points[:, :, offset+1].clamp(0, h-1)
            index = (y*w+x).unsqueeze(1).expand(-1, c, -1)
            selected.append(flat.gather(2, index).transpose(1, 2))
        features = torch.cat([context[:, None, :].expand(-1, points.shape[1], -1),
                              *selected, data["candidates"]], dim=-1)
        logits = self.scorer(features).squeeze(-1).masked_fill(~data["mask"], -1e9)
        return logits, self.value(context).squeeze(-1)

    @torch.no_grad()
    def act(self, observation, generator=None):
        logits, values = self(batch([observation]))
        log_probs = logits.log_softmax(-1)
        selected = torch.multinomial(log_probs.exp()[0], 1, generator=generator).item()
        return selected, log_probs[0, selected].item(), values[0].item()

    @property
    def parameter_count(self):
        return sum(p.numel() for p in self.parameters() if p.requires_grad)


def advantages(rewards, values, dones, last_value, gamma, lam):
    result = np.zeros(len(rewards), dtype=np.float32)
    carry = 0.0
    for t in reversed(range(len(rewards))):
        continuation = 0.0 if dones[t] else 1.0
        next_value = last_value if t == len(rewards)-1 else values[t+1]
        delta = rewards[t] + gamma*next_value*continuation - values[t]
        carry = delta + gamma*lam*continuation*carry
        result[t] = carry
    return result, result+np.asarray(values, dtype=np.float32)


def update(model, optimizer, rollout, config, rng):
    advantage, returns = advantages(rollout["rewards"], rollout["values"], rollout["dones"], 0.0,
                                    config["gamma"], config["gae_lambda"])
    advantage = (advantage-advantage.mean())/(advantage.std()+1e-8)
    totals = {"policy_loss": 0.0, "value_loss": 0.0, "entropy": 0.0, "approx_kl": 0.0, "gradient_norm": 0.0}
    batches = 0
    seconds = {key: 0.0 for key in ("batch", "forward_loss", "backward", "optimizer")}
    for _ in range(config["epochs"]):
        permutation = rng.permutation(len(advantage))
        for start in range(0, len(advantage), config["minibatch"]):
            indices = permutation[start:start+config["minibatch"]]
            started = time.perf_counter()
            data = batch([rollout["observations"][i] for i in indices])
            seconds["batch"] += time.perf_counter()-started
            started = time.perf_counter()
            logits, value = model(data)
            log_probs = logits.log_softmax(-1)
            selected = torch.tensor([rollout["actions"][i] for i in indices], dtype=torch.long)
            log_prob = log_probs.gather(1, selected[:, None]).squeeze(1)
            old = torch.tensor([rollout["log_probs"][i] for i in indices])
            ratio = (log_prob-old).exp()
            adv = torch.from_numpy(advantage[indices])
            policy_loss = torch.maximum(-adv*ratio, -adv*ratio.clamp(1-config["clip"], 1+config["clip"])).mean()
            value_loss = (value-torch.from_numpy(returns[indices])).square().mean()
            entropy = -(log_probs.exp()*log_probs).sum(-1).mean()
            loss = policy_loss+config["value_coefficient"]*value_loss-config["entropy_coefficient"]*entropy
            if not torch.isfinite(loss):
                raise RuntimeError("learning.nonfinite_loss: PPO loss is not finite")
            optimizer.zero_grad(set_to_none=True)
            seconds["forward_loss"] += time.perf_counter()-started
            started = time.perf_counter()
            loss.backward()
            seconds["backward"] += time.perf_counter()-started
            started = time.perf_counter()
            grad = torch.nn.utils.clip_grad_norm_(model.parameters(), config["gradient_norm"], error_if_nonfinite=True)
            optimizer.step()
            seconds["optimizer"] += time.perf_counter()-started
            values = {"policy_loss": policy_loss.item(), "value_loss": value_loss.item(), "entropy": entropy.item(),
                      "approx_kl": ((ratio-1)-(log_prob-old)).mean().item(), "gradient_norm": float(grad)}
            for key in totals:
                totals[key] += values[key]
            batches += 1
    return {**{key: value/batches for key, value in totals.items()}, "optimizer_steps": batches,
            "seconds": seconds}
