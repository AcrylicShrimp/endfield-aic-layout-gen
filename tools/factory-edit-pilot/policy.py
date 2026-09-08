"""CNN plus per-object MLP, CPU action sampling, and CPU/CUDA PPO updates."""

import numpy as np
import torch
from torch import nn
import time


class MaskedBatchNorm2d(nn.BatchNorm2d):
    """Calibrate on real cells; all policy/gradient forwards use frozen statistics."""

    def forward(self, data, valid, calibrate=False):
        if calibrate:
            count = valid.sum()
            mean = (data * valid).sum(dim=(0, 2, 3)) / count
            variance = ((data - mean[None, :, None, None]).square() * valid).sum(dim=(0, 2, 3)) / count
            if count <= 1 or not torch.isfinite(mean).all() or not torch.isfinite(variance).all():
                raise RuntimeError("normalization.nonfinite: calibration requires at least two finite valid cells")
            with torch.no_grad():
                self.running_mean.lerp_(mean, self.momentum)
                self.running_var.lerp_(variance * count / (count - 1), self.momentum)
                self.num_batches_tracked.add_(1)
            result = (data - mean[None, :, None, None]) * torch.rsqrt(variance[None, :, None, None] + self.eps)
            return result * self.weight[None, :, None, None] + self.bias[None, :, None, None]
        return nn.functional.batch_norm(data, self.running_mean, self.running_var,
                                        self.weight, self.bias, training=False, eps=self.eps)


def batch(observations):
    if not observations or any(o.get("schema_version") != 2 for o in observations):
        raise ValueError("policy.observation_schema: schema 2 semantic entities are required")
    count = len(observations)
    first = observations[0]
    height = max(o["raster"].shape[1] for o in observations)
    width = max(o["raster"].shape[2] for o in observations)
    candidates = max(len(o["candidates"]) for o in observations)
    entities = max(len(o["entities"]) for o in observations)
    edges = max(len(o["edges"]) for o in observations)
    arrays = {
        "raster": np.zeros((count, first["raster"].shape[0], height, width), dtype=np.float32),
        "entity_maps": np.full((count, first["entity_maps"].shape[0], height, width), -1, dtype=np.int64),
        "entities": np.zeros((count, entities, first["entities"].shape[1]), dtype=np.float32),
        "entity_mask": np.zeros((count, entities), dtype=bool),
        "edges": np.zeros((count, edges, 2), dtype=np.int64),
        "edge_types": np.zeros((count, edges), dtype=np.int64),
        "edge_mask": np.zeros((count, edges), dtype=bool),
        "candidates": np.zeros((count, candidates, first["candidates"].shape[1]), dtype=np.float32),
        "operands": np.zeros((count, candidates, *first["operands"].shape[1:]), dtype=np.float32),
        "operand_entities": np.full((count, candidates, 2), -1, dtype=np.int64),
        "operand_mask": np.zeros((count, candidates, 2), dtype=bool),
        "points": np.zeros((count, candidates, 2, 2), dtype=np.int64),
        "mask": np.zeros((count, candidates), dtype=bool),
        "stats": np.stack([o["stats"] for o in observations]),
    }
    for i, observation in enumerate(observations):
        _, h, w = observation["raster"].shape
        n, e, l = len(observation["candidates"]), len(observation["entities"]), len(observation["edges"])
        for key in ("raster", "entity_maps"):
            arrays[key][i, :, :h, :w] = observation[key]
        arrays["entities"][i, :e] = observation["entities"]
        arrays["entity_mask"][i, :e] = True
        for key in ("edges", "edge_types"):
            arrays[key][i, :l] = observation[key]
        arrays["edge_mask"][i, :l] = True
        for key in ("candidates", "operands", "operand_entities", "operand_mask", "points"):
            arrays[key][i, :n] = observation[key]
        arrays["mask"][i, :n] = True
    return {key: torch.from_numpy(value) for key, value in arrays.items()}


def single(observation):
    if observation.get("schema_version") != 2:
        raise ValueError("policy.observation_schema: schema 2 semantic entities are required")
    data = {key: torch.from_numpy(value).unsqueeze(0) for key, value in observation.items() if key != "schema_version"}
    data.update(mask=torch.ones((1, len(observation["candidates"])), dtype=torch.bool),
                entity_mask=torch.ones((1, len(observation["entities"])), dtype=torch.bool),
                edge_mask=torch.ones((1, len(observation["edges"])), dtype=torch.bool))
    return data


def gather_entities(entities, indices):
    count, _, width = entities.shape
    flat = indices.clamp_min(0).reshape(count, -1)
    gathered = entities.gather(1, flat[:, :, None].expand(-1, -1, width))
    return gathered.reshape(*indices.shape, width) * (indices >= 0).unsqueeze(-1)


class Policy(nn.Module):
    def __init__(self, example, normalization="none", width_multiplier=1):
        super().__init__()
        if example.get("schema_version") != 2:
            raise ValueError("policy.observation_schema: schema 2 semantic entities are required")
        if normalization not in ("none", "batch"):
            raise ValueError(f"policy.normalization: unknown normalization {normalization!r}")
        if type(width_multiplier) is not int or width_multiplier not in (1, 2):
            raise ValueError("policy.width: width_multiplier must be 1 or 2")
        self.width_multiplier, self.normalization = width_multiplier, normalization
        self.dimensions = {"schema_version": 2, "raster_channels": example["raster"].shape[0],
                           "entity_maps": example["entity_maps"].shape[0], "entity_features": example["entities"].shape[1],
                           "edge_types": 8, "operand_features": example["operands"].shape[2], "operand_slots": 2,
                           "candidate_features": example["candidates"].shape[1], "stats": len(example["stats"])}
        d = self.dimensions
        small, spatial, hidden, projected = 16*width_multiplier, 32*width_multiplier, 64*width_multiplier, 8*width_multiplier
        self.entity_encoder = nn.Sequential(nn.Linear(d["entity_features"], spatial), nn.ReLU(), nn.Linear(spatial, spatial), nn.ReLU())
        self.message = nn.Sequential(nn.Linear(spatial+8, spatial), nn.ReLU(), nn.Linear(spatial, spatial))
        self.entity_update = nn.Sequential(nn.Linear(2*spatial, spatial), nn.ReLU(), nn.Linear(spatial, spatial))
        self.spatial_projection = nn.Linear(spatial, projected)
        self.convolutions = nn.ModuleList([nn.Conv2d(d["raster_channels"]+d["entity_maps"]*projected, small, 3, padding=1),
                                          nn.Conv2d(small, spatial, 3, padding=1), nn.Conv2d(spatial, spatial, 3, padding=1)])
        self.fusion = nn.Sequential(nn.Linear(3*spatial+d["stats"], hidden), nn.ReLU(), nn.Linear(hidden, hidden), nn.ReLU())
        self.operand_encoder = nn.Sequential(nn.Linear(2*spatial+d["operand_features"], hidden), nn.ReLU(),
                                             nn.Linear(hidden, hidden), nn.ReLU())
        self.scorer = nn.Sequential(nn.Linear(2*hidden+d["candidate_features"], hidden), nn.ReLU(),
                                   nn.Linear(hidden, hidden), nn.ReLU(), nn.Linear(hidden, 1))
        self.value = nn.Sequential(nn.Linear(hidden, hidden), nn.ReLU(), nn.Linear(hidden, 1))
        nn.init.orthogonal_(self.scorer[-1].weight, gain=0.01)
        nn.init.zeros_(self.scorer[-1].bias)
        self.normalizations = nn.ModuleList([MaskedBatchNorm2d(n) for n in (small, spatial, spatial)] if normalization == "batch" else [])

    def load_state_dict(self, state_dict, *args, **kwargs):
        if "objects.0.weight" in state_dict:
            raise ValueError("policy.checkpoint_schema: legacy fixed-entity policy requires its frozen source revision")
        return super().load_state_dict(state_dict, *args, **kwargs)

    def forward(self, data, calibrate=False):
        node_mask = data["entity_mask"].unsqueeze(-1)
        entities = self.entity_encoder(data["entities"]) * node_mask
        sender, receiver = data["edges"].unbind(-1)
        relation = nn.functional.one_hot(data["edge_types"], 8).to(entities.dtype)
        edge_mask = data["edge_mask"].unsqueeze(-1)
        receiver_index = receiver[:, :, None].expand(-1, -1, entities.shape[-1])
        degree = torch.zeros_like(entities[:, :, :1]).scatter_add(1, receiver[:, :, None], edge_mask.to(entities.dtype))
        for _ in range(4):
            messages = self.message(torch.cat([gather_entities(entities, sender), relation], dim=-1)) * edge_mask
            aggregated = torch.zeros_like(entities).scatter_add(1, receiver_index, messages) / degree.clamp_min(1)
            entities = torch.relu(entities+self.entity_update(torch.cat([entities, aggregated], dim=-1))) * node_mask
        maps = data["entity_maps"]
        mapped = gather_entities(self.spatial_projection(entities), maps)
        mapped = mapped.permute(0, 1, 4, 2, 3).flatten(1, 2)
        valid = data["raster"][:, -1:]
        spatial = torch.cat([data["raster"], mapped], dim=1)
        for index, layer in enumerate(self.convolutions):
            spatial = layer(spatial)
            if self.normalization == "batch":
                spatial = self.normalizations[index](spatial, valid, calibrate=calibrate)
            spatial = torch.relu(spatial)*valid
        pooled = spatial.sum(dim=(2, 3))/valid.sum(dim=(2, 3)).clamp_min(1)
        mean = entities.sum(dim=1)/node_mask.sum(dim=1).clamp_min(1)
        maximum = entities.masked_fill(~node_mask, -torch.inf).max(dim=1).values
        context = self.fusion(torch.cat([pooled, mean, maximum, data["stats"]], dim=-1))
        points = data["points"]
        count, channels, height, width = spatial.shape
        x, y = points.unbind(-1)
        inside = (x >= 0) & (x < width) & (y >= 0) & (y < height)
        indices = (y.clamp(0, height-1)*width+x.clamp(0, width-1)).reshape(count, -1)
        sampled = spatial.flatten(2).gather(2, indices[:, None, :].expand(-1, channels, -1))
        sampled = sampled.transpose(1, 2).reshape(*x.shape, channels) * inside.unsqueeze(-1)
        operands = self.operand_encoder(torch.cat([gather_entities(entities, data["operand_entities"]), sampled, data["operands"]], dim=-1))
        operand_mask = data["operand_mask"].unsqueeze(-1)
        operands = (operands*operand_mask).sum(dim=2)/operand_mask.sum(dim=2).clamp_min(1)
        features = torch.cat([context[:, None, :].expand(-1, operands.shape[1], -1), operands, data["candidates"]], dim=-1)
        logits = self.scorer(features).squeeze(-1).masked_fill(~data["mask"], -1e9)
        return logits, self.value(context).squeeze(-1)

    @torch.no_grad()
    def calibrate_normalization(self, observations, minibatch):
        if self.normalization == "none":
            return {"kind": "none", "observations": 0, "batches": 0}
        device = next(self.parameters()).device
        batches = 0
        for start in range(0, len(observations), minibatch):
            data = {key: value.to(device) for key, value in batch(observations[start:start+minibatch]).items()}
            self(data, calibrate=True)
            batches += 1
        return {"kind": "batch", "observations": len(observations), "batches": batches}

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
        self.extents = np.asarray([(o["raster"].shape[1], o["raster"].shape[2], len(o["candidates"]),
                                    len(o["entities"]), len(o["edges"])) for o in observations])
        h, w, n, e, l = self.extents.max(axis=0).tolist()
        first = observations[0]
        count = len(observations)
        self.estimated_bytes = count*((first["raster"].shape[0]*4+first["entity_maps"].shape[0]*8)*h*w
                                     + e*(first["entities"].shape[1]*4+1)+l*(2*8+8+1)
                                     + n*(first["candidates"].shape[1]*4+2*(first["operands"].shape[2]*4+8+1+2*8)+1)
                                     + first["stats"].nbytes+8+3*4)
        self.tensors = None
        if self.estimated_bytes <= maximum_bytes:
            self.tensors = {key: value.to(device) for key, value in batch(observations).items()}
            self.tensors.update(actions=torch.tensor(rollout["actions"], dtype=torch.long, device=device),
                                old=torch.tensor(rollout["log_probs"], device=device),
                                advantage=torch.from_numpy(advantage).to(device),
                                returns=torch.from_numpy(returns).to(device))

    def select(self, indices):
        h, w, n, e, l = self.extents[indices].max(axis=0).tolist()
        device = self.tensors["raster"].device
        selection = torch.as_tensor(indices, device=device)
        result = {}
        for key, value in self.tensors.items():
            if key in ("raster", "entity_maps"):
                value = value[:, :, :h, :w]
            elif key in ("candidates", "points", "mask", "operands", "operand_entities", "operand_mask"):
                value = value[:, :n]
            elif key in ("entities", "entity_mask"):
                value = value[:, :e]
            elif key in ("edges", "edge_types", "edge_mask"):
                value = value[:, :l]
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
    started = time.perf_counter()
    calibration = model.calibrate_normalization(rollout["observations"], config["minibatch"])
    calibration["seconds"] = completed_time()-started
    return {**{key: value/batches for key, value in totals.items()}, "optimizer_steps": batches,
            "normalization_calibration": calibration,
            "seconds": seconds,
            "preparation": {"mode": "rollout_device" if prepared is not None and prepared.tensors is not None else "minibatch",
                            "maximum_bytes": preparation_budget,
                            "estimated_rollout_bytes": prepared.estimated_bytes if prepared is not None else 0}}
