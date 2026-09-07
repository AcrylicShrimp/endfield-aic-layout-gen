"""CNN plus per-object MLP, CPU action sampling, and CPU/CUDA PPO updates."""

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


def single(observation):
    """Read-only zero-copy input views; the forward pass never modifies observations."""
    return {**{key: torch.from_numpy(value).unsqueeze(0) for key, value in observation.items()},
            "mask": torch.ones((1, len(observation["candidates"])), dtype=torch.bool)}


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
    def act(self, observation, generator=None, forward=None):
        logits, values = (self if forward is None else forward)(single(observation))
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


class PreparedRollout:
    """A bounded device copy, cropped back to each unchanged minibatch's exact shape."""

    def __init__(self, rollout, advantage, returns, device, maximum_bytes=1 << 30):
        if type(maximum_bytes) is not int or maximum_bytes < 0:
            raise ValueError("preparation.invalid_budget: maximum_bytes must be a nonnegative integer")
        observations = rollout["observations"]
        self.extents = np.asarray([(o["raster"].shape[1], o["raster"].shape[2], len(o["candidates"]))
                                   for o in observations])
        h, w, n = self.extents.max(axis=0).tolist()
        first = observations[0]
        count = len(observations)
        self.estimated_bytes = count*(first["raster"].shape[0]*h*w*4
                                     + n*(first["candidates"].shape[1]*4+4*8+1)
                                     + first["objects"].nbytes+first["stats"].nbytes+8+3*4)
        self.tensors = None
        if self.estimated_bytes <= maximum_bytes:
            self.tensors = {key: value.to(device) for key, value in batch(observations).items()}
            self.tensors.update(actions=torch.tensor(rollout["actions"], dtype=torch.long, device=device),
                                old=torch.tensor(rollout["log_probs"], device=device),
                                advantage=torch.from_numpy(advantage).to(device),
                                returns=torch.from_numpy(returns).to(device))

    def select(self, indices):
        h, w, n = self.extents[indices].max(axis=0).tolist()
        device = self.tensors["raster"].device
        selection = torch.as_tensor(indices, device=device)
        result = {}
        for key, value in self.tensors.items():
            if key == "raster":
                value = value[:, :, :h, :w]
            elif key in ("candidates", "points", "mask"):
                value = value[:, :n]
            result[key] = value.index_select(0, selection)
        return result


def update(model, optimizer, rollout, config, rng):
    device = next(model.parameters()).device

    def completed_time():
        # Stage clocks include completed device work, not just asynchronous dispatch.
        if device.type == "cuda":
            torch.cuda.synchronize(device)
        return time.perf_counter()

    advantage, returns = advantages(rollout["rewards"], rollout["values"], rollout["dones"], 0.0,
                                    config["gamma"], config["gae_lambda"])
    advantage = (advantage-advantage.mean())/(advantage.std()+1e-8)
    totals = {"policy_loss": 0.0, "value_loss": 0.0, "entropy": 0.0, "approx_kl": 0.0, "gradient_norm": 0.0}
    batches = 0
    seconds = {key: 0.0 for key in ("batch", "forward_loss", "backward", "optimizer")}
    prepared = None
    preparation_budget = config.get("prepared_rollout_max_bytes", 1 << 30)
    if device.type == "cuda":
        started = time.perf_counter()
        prepared = PreparedRollout(rollout, advantage, returns, device, maximum_bytes=preparation_budget)
        seconds["batch"] += completed_time()-started
    for _ in range(config["epochs"]):
        permutation = rng.permutation(len(advantage))
        for start in range(0, len(advantage), config["minibatch"]):
            indices = permutation[start:start+config["minibatch"]]
            started = time.perf_counter()
            if prepared is not None and prepared.tensors is not None:
                data = prepared.select(indices)
                selected, old, adv, target = (data[k] for k in ("actions", "old", "advantage", "returns"))
            else:
                data = batch([rollout["observations"][i] for i in indices])
                data = {key: value.to(device) for key, value in data.items()}
                selected = torch.tensor([rollout["actions"][i] for i in indices], dtype=torch.long, device=device)
                old = torch.tensor([rollout["log_probs"][i] for i in indices], device=device)
                adv = torch.from_numpy(advantage[indices]).to(device)
                target = torch.from_numpy(returns[indices]).to(device)
            seconds["batch"] += completed_time()-started
            started = time.perf_counter()
            logits, value = model(data)
            log_probs = logits.log_softmax(-1)
            log_prob = log_probs.gather(1, selected[:, None]).squeeze(1)
            ratio = (log_prob-old).exp()
            policy_loss = torch.maximum(-adv*ratio, -adv*ratio.clamp(1-config["clip"], 1+config["clip"])).mean()
            value_loss = (value-target).square().mean()
            entropy = -(log_probs.exp()*log_probs).sum(-1).mean()
            loss = policy_loss+config["value_coefficient"]*value_loss-config["entropy_coefficient"]*entropy
            if not torch.isfinite(loss):
                raise RuntimeError("learning.nonfinite_loss: PPO loss is not finite")
            optimizer.zero_grad(set_to_none=True)
            seconds["forward_loss"] += completed_time()-started
            started = time.perf_counter()
            loss.backward()
            seconds["backward"] += completed_time()-started
            started = time.perf_counter()
            grad = torch.nn.utils.clip_grad_norm_(model.parameters(), config["gradient_norm"], error_if_nonfinite=True)
            optimizer.step()
            seconds["optimizer"] += completed_time()-started
            values = {"policy_loss": policy_loss.item(), "value_loss": value_loss.item(), "entropy": entropy.item(),
                      "approx_kl": ((ratio-1)-(log_prob-old)).mean().item(), "gradient_norm": float(grad)}
            for key in totals:
                totals[key] += values[key]
            batches += 1
    return {**{key: value/batches for key, value in totals.items()}, "optimizer_steps": batches,
            "seconds": seconds,
            "preparation": {"mode": "rollout_device" if prepared is not None and prepared.tensors is not None else "minibatch",
                            "maximum_bytes": preparation_budget,
                            "estimated_rollout_bytes": prepared.estimated_bytes if prepared is not None else 0}}
