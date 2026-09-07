"""Conditional direct action decoder: no complete mutation candidate list."""

import math

import torch
from torch import nn
from torch.nn import functional as F

KINDS = ["relocate", "pose", "port", "reroute", "compact"]
STAGES = ["kind", "member", "rotation", "x", "y", "terminal", "port", "flag"]


class DirectPolicy(nn.Module):
    def __init__(self, width=32):
        super().__init__()
        self.config = dict(width=width)
        self.token = nn.Sequential(nn.Linear(17, width), nn.ReLU())
        self.message = nn.ModuleList(nn.Linear(width * 3, width) for _ in range(2))
        self.norm = nn.ModuleList(nn.LayerNorm(width) for _ in range(2))
        self.grid = nn.Sequential(nn.Conv2d(10, width, 3, padding=1), nn.ReLU(), nn.Conv2d(width, width, 3, padding=1), nn.ReLU())
        self.query = nn.Sequential(nn.Linear(32, width), nn.ReLU())
        self.attention = nn.MultiheadAttention(width, 2, dropout=0, batch_first=True)
        self.head = nn.Sequential(nn.Linear(width * 5, width), nn.ReLU(), nn.Linear(width, 1))

    def encode(self, obs):
        s = obs["state"]
        rates = {}
        for t in s["terminals"]:
            if t["endpoint"]["kind"] == "facility":
                identity = t["endpoint"]["instance"]
                rates[identity] = rates.get(identity, 0) + t["rate"]["numerator"] / t["rate"]["denominator"]
        extra = []
        for i in range(len(s["tokens"])):
            if i < len(s["facilities"]):
                identity = s["facilities"][i]["instance"]
                u = s["utilizations"][identity]
                extra.append([u["numerator"] / u["denominator"], math.log1p(rates.get(identity, 0))])
            else:
                extra.append([0, 0])
        tokens = torch.tensor([a + b for a, b in zip(s["tokens"], extra)], dtype=torch.float32)
        adjacency = torch.tensor(s["adjacency"], dtype=torch.float32)
        h = self.token(tokens)
        for message, norm in zip(self.message, self.norm):
            incoming = adjacency.T @ h / adjacency.T.sum(-1, keepdim=True).clamp_min(1)
            outgoing = adjacency @ h / adjacency.sum(-1, keepdim=True).clamp_min(1)
            h = norm(h + torch.relu(message(torch.cat([h, incoming, outgoing], -1))))
        b = s["bounds"]
        grid = torch.tensor(s["grid"], dtype=torch.float32).reshape(1, 10, b["height"], b["width"])
        spatial = self.grid(grid)[0]
        return h, spatial, spatial.mean((1, 2))

    def logits(self, encoded, rows, context, selected):
        h, spatial, pooled = encoded
        q = self.query(torch.tensor([r + context for r in rows], dtype=torch.float32))
        attended, _ = self.attention(q[None], h[None], h[None], need_weights=False)
        positions = [(min(spatial.shape[2] - 1, max(0, int(r[13] * spatial.shape[2]))),
                      min(spatial.shape[1] - 1, max(0, int(r[14] * spatial.shape[1])))) for r in rows]
        local = torch.stack([spatial[:, y, x] for x, y in positions])
        parent = h[selected].mean(0) if selected else h.mean(0)
        return self.head(torch.cat([q, attended[0], local, pooled.expand(len(rows), -1), parent.expand(len(rows), -1)], -1)).squeeze(-1)


def location_mask(state, members, rotation=None):
    """Exact facility-body legality for a chosen group/rotation, not a promising-pose list."""
    b = state["bounds"]
    ps = [state["facilities"][i] for i in members]
    xmin, ymin = min(p["x"] for p in ps), min(p["y"] for p in ps)
    shapes = [(p["x"] - xmin, p["y"] - ymin, p["width"], p["height"]) for p in ps]
    if rotation is not None and (rotation - ps[0]["rotation"]) % 180:
        shapes = [(0, 0, ps[0]["height"], ps[0]["width"])]
    width = max(x + w for x, y, w, h in shapes)
    height = max(y + h for x, y, w, h in shapes)
    if width > b["width"] or height > b["height"]:
        return None
    occupied = torch.tensor(state["grid"][0], dtype=torch.float32).reshape(b["height"], b["width"])
    for p in ps:
        x, y = p["x"] - b["minX"], p["y"] - b["minY"]
        occupied[y:y + p["height"], x:x + p["width"]] = 0
    footprint = torch.zeros(height, width)
    for x, y, w, h in shapes:
        footprint[y:y + h, x:x + w] = 1
    with torch.no_grad():
        legal = F.conv2d(occupied[None, None], footprint[None, None])[0, 0] < .5
    return dict(legal=legal, xmin=xmin, ymin=ymin, width=width, height=height)


def decode(model, obs, *, generator=None, decisions=None, uniform=False):
    s, b = obs["state"], obs["state"]["bounds"]
    encoded = None if uniform else model.encode(obs)
    selected, recorded, logps, entropies = [], [], [], []
    kind_index = None

    def row(stage, *, kind=None, x=0, y=0, width=0, height=0, dx=0, dy=0, rotation=0, rate=0, direction=0, transport=0):
        op = kind_index if kind is None else kind
        return ([float(stage == name) for name in STAGES] + [float(op == i) for i in range(len(KINDS))]
                + [(x - b["minX"]) / b["width"], (y - b["minY"]) / b["height"], width / b["width"], height / b["height"],
                   dx / b["width"], dy / b["height"], math.cos(rotation * math.pi / 180), math.sin(rotation * math.pi / 180),
                   rate, direction, transport])

    def choose(rows, mask=None):
        logits = torch.zeros(len(rows)) if uniform else model.logits(encoded, rows, obs["context"], selected)
        if mask is not None:
            logits = logits.masked_fill(~torch.tensor(mask, dtype=torch.bool), -torch.inf)
        if not torch.isfinite(logits).any():
            raise ValueError("Direct decoder has no legal partial choice")
        dist = torch.distributions.Categorical(logits=logits)
        index = decisions[len(recorded)] if decisions is not None else int(torch.multinomial(dist.probs, 1, generator=generator))
        if not 0 <= index < len(rows) or not torch.isfinite(logits[index]):
            raise ValueError("Replay decision is outside its legal vocabulary")
        recorded.append(index)
        logps.append(dist.log_prob(torch.tensor(index)))
        entropies.append(dist.entropy())
        return index

    occupied_ports = {(t["endpoint"]["instance"], t["endpoint"]["port"]) for t in s["terminals"] if t["endpoint"]["kind"] == "facility"}
    facility_index = {p["instance"]: i for i, p in enumerate(s["facilities"])}
    terminal_options = []
    for t in s["terminals"]:
        if t["endpoint"]["kind"] != "facility":
            continue
        facility = s["facilities"][facility_index[t["endpoint"]["instance"]]]
        ports = []
        for p in facility["ports"]:
            x, y = p["connection"]["x"], p["connection"]["y"]
            if (p["direction"] == t["direction"] and p["transport"] == t["transport"]
                    and (facility["instance"], p["id"]) not in occupied_ports
                    and b["minX"] <= x <= b["maxX"] and b["minY"] <= y <= b["maxY"]
                    and not s["grid"][0][(y - b["minY"]) * b["width"] + x - b["minX"]]):
                ports.append(p)
        if ports:
            terminal_options.append((t, ports))
    kind_index = choose([row("kind", kind=i) for i in range(len(KINDS))], [True, True, bool(terminal_options), bool(s["chains"]), True])
    kind = KINDS[kind_index]
    action = dict(kind=kind, compact_after=False)
    if kind in ("relocate", "pose"):
        rows = [row("member", **{k: p[k] for k in ("x", "y", "width", "height", "rotation")}) for p in s["facilities"]]
        selected.append(choose(rows))
        if kind == "relocate":
            neighbors = {i: set() for i in range(len(rows))}
            for e in s["edges"]:
                a, z = facility_index[e["source"]], facility_index[e["target"]]
                neighbors[a].add(z)
                neighbors[z].add(a)
            while len(selected) < min(s["max_members"], len(rows)):
                frontier = set().union(*(neighbors[i] for i in selected)) - set(selected)
                index = choose(rows + [row("member", rate=-1, direction=-1)], [i in frontier for i in range(len(rows))] + [True])
                if index == len(rows):
                    break
                selected.append(index)
            location = location_mask(s, selected)
        else:
            p = s["facilities"][selected[0]]
            locations = [location_mask(s, selected, rotation=r) for r in p["rotations"]]
            index = choose([row("rotation", x=p["x"], y=p["y"], rotation=r) for r in p["rotations"]],
                           [loc is not None and bool(loc["legal"].any()) for loc in locations])
            location = locations[index]
            action["rotation"] = p["rotations"][index]
        legal, xmin, ymin = location["legal"], location["xmin"], location["ymin"]
        common = dict(width=location["width"], height=location["height"], rotation=action.get("rotation", 0))
        x = choose([row("x", x=b["minX"] + i, y=ymin, dx=b["minX"] + i - xmin, **common) for i in range(legal.shape[1])], legal.any(0).tolist())
        y = choose([row("y", x=b["minX"] + x, y=b["minY"] + i, dx=b["minX"] + x - xmin, dy=b["minY"] + i - ymin, **common) for i in range(legal.shape[0])], legal[:, x].tolist())
        action.update(instances=[s["facilities"][i]["instance"] for i in selected], dx=b["minX"] + x - xmin, dy=b["minY"] + y - ymin)
    elif kind == "port":
        t, ports = terminal_options[choose([row("terminal", x=t["position"]["x"], y=t["position"]["y"],
                                               rate=math.log1p(t["rate"]["numerator"] / t["rate"]["denominator"]),
                                               direction=1 if t["direction"] == "input" else -1, transport=1 if t["transport"] == "belt" else -1)
                                           for t, _ in terminal_options])]
        selected.append(facility_index[t["endpoint"]["instance"]])
        p = ports[choose([row("port", x=p["connection"]["x"], y=p["connection"]["y"],
                              dx=p["connection"]["x"] - t["position"]["x"], dy=p["connection"]["y"] - t["position"]["y"]) for p in ports])]
        action.update(terminal=t["id"], port=p["id"])
    elif kind == "reroute":
        chain = s["chains"][choose([row("terminal", x=c["start"]["x"], y=c["start"]["y"], dx=c["end"]["x"] - c["start"]["x"],
                                         dy=c["end"]["y"] - c["start"]["y"], rate=math.log1p(c["cells"]), transport=1 if c["transport"] == "belt" else -1) for c in s["chains"]])]
        order = choose([row("flag", direction=-1), row("flag", direction=1)])
        action.update(chain=chain["id"], order=["near", "far"][order])
    if kind != "compact":
        action["compact_after"] = bool(choose([row("flag", rate=0), row("flag", rate=1)]))
    if decisions is not None and len(recorded) != len(decisions):
        raise ValueError("Replay decision count changed")
    return action, recorded, torch.stack(logps).sum(), torch.stack(entropies).mean()


def checkpoint(model):
    return dict(schema_version=1, architecture=model.config, weights={k: v.detach().tolist() for k, v in model.state_dict().items()})


def restore(value):
    if value["schema_version"] != 1:
        raise ValueError("Unsupported direct checkpoint")
    model = DirectPolicy(**value["architecture"])
    model.load_state_dict({k: torch.tensor(v) for k, v in value["weights"].items()}, strict=True)
    if not all(torch.isfinite(v).all() for v in model.state_dict().values()):
        raise ValueError("Nonfinite direct checkpoint")
    return model
