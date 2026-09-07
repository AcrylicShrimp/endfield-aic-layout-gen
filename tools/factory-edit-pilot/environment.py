"""Explicit local edits; no route search, repair, or outcome-ranked candidates."""

from collections import Counter, defaultdict
from copy import deepcopy
import json
import math
import time

import numpy as np

from acceptance import DIRECTIONS, SIDES, check


OPERATIONS = ("SET_PLACEMENTS", "UNBIND_TERMINALS", "BIND_TERMINALS", "ERASE_TRANSPORT",
              "EXTEND", "CONNECT", "DISCARD_EDIT")


class Mechanical(ValueError):
    pass


class SpatialGuard(RuntimeError):
    pass


def cells_on_line(a, b):
    a, b = tuple(a), tuple(b)
    if a[0] != b[0] and a[1] != b[1]:
        raise Mechanical("A segment must be cardinal")
    dx = (b[0] > a[0]) - (b[0] < a[0])
    dy = (b[1] > a[1]) - (b[1] < a[1])
    length = abs(b[0]-a[0]) + abs(b[1]-a[1])
    return [(a[0]+i*dx, a[1]+i*dy) for i in range(length+1)]


def initial_layout(catalog):
    state = {"schema_version": 1, "placements": {}, "bindings": {}, "networks": {}}
    state["placements"] = {fid: list(f["pose"]) for fid, f in catalog.facilities.items()}
    for nid, n in catalog.networks.items():
        path = []
        for a, b in zip(n["corners"], n["corners"][1:]):
            segment = cells_on_line(a, b)
            path.extend(segment if not path else segment[1:])
        if len(n["corners"]) == 1:
            path = [tuple(n["corners"][0])]
        state["networks"][nid] = {"cells": set(path), "edges": set(zip(path, path[1:]))}
        for role in ("source", "target"):
            state["bindings"][f"{nid}:{role}"] = deepcopy(n[f"{role}_binding"])
    return state


def serialize(state):
    return {"schema_version": 1, "placements": deepcopy(state["placements"]),
            "bindings": deepcopy(state["bindings"]), "networks": {
                nid: {"cells": [list(c) for c in sorted(n["cells"])],
                      "edges": [[list(a), list(b)] for a, b in sorted(n["edges"])]}
                for nid, n in state["networks"].items()}}


def clone(state):
    return {"schema_version": 1, "placements": {k: list(v) for k, v in state["placements"].items()},
            "bindings": {k: None if v is None else dict(v) for k, v in state["bindings"].items()},
            "networks": {k: {"cells": set(v["cells"]), "edges": set(v["edges"])} for k, v in state["networks"].items()}}


class Environment:
    def __init__(self, catalog, config):
        self.catalog, self.config = catalog, config
        self.fids, self.nids, self.tids = list(catalog.facilities), list(catalog.networks), catalog.terminal_ids
        self.root = initial_layout(catalog)
        admission = check(catalog, serialize(self.root))
        if not admission["valid"]:
            raise Mechanical(f"Initial fixture {catalog.path} failed admission: {admission['diagnostics']}")
        self.initial_score = admission["score"]
        self.scale = math.sqrt(self.initial_score[0])
        self.counters = Counter()
        self.seconds = Counter()
        self.reset()

    def reset(self):
        self.draft, self.base, self.best = clone(self.root), clone(self.root), clone(self.root)
        self.base_score = list(self.initial_score)
        self.best_score = list(self.initial_score)
        self.steps, self.revision = 0, 0
        self.trace = []
        self.episode_counts = Counter()
        return self

    def body(self, fid, pose):
        definition = self.catalog.facility_type(fid)
        w, h = definition["footprint"]["width"], definition["footprint"]["height"]
        if pose[2] in (90, 270):
            w, h = h, w
        return {(pose[0]+x, pose[1]+y) for x in range(w) for y in range(h)}

    def location(self, state, tid, binding=None):
        if binding is None:
            binding = state["bindings"][tid]
        if binding is None:
            return None
        nid, role, endpoint = self.catalog.terminal(tid)
        if "external" in endpoint:
            return tuple(binding["cell"]), DIRECTIONS[binding["side"]]
        fid = endpoint["facility"]
        f = self.catalog.facility_type(fid)
        p = next(p for p in f["ports"] if p["id"] == binding["port"])
        x, y, r = state["placements"][fid]
        u, v = p["position"]["x"], p["position"]["y"]
        w, h = f["footprint"]["width"], f["footprint"]["height"]
        uv = {0: (u, v), 90: (h-1-v, u), 180: (w-1-u, h-1-v), 270: (v, w-1-u)}[r]
        edge = SIDES[(SIDES.index(p["edge"][0].upper())+r//90) % 4]
        dx, dy = DIRECTIONS[edge]
        return (x+uv[0]+dx, y+uv[1]+dy), (-dx, -dy)

    def index(self, state):
        bodies = {fid: self.body(fid, pose) for fid, pose in state["placements"].items()}
        occupied = set().union(*bodies.values())
        for n in state["networks"].values():
            occupied.update(n["cells"])
        incoming, outgoing = {}, {}
        protected = {nid: set() for nid in self.nids}
        for nid, n in state["networks"].items():
            ins, outs = defaultdict(list), defaultdict(list)
            for a, b in n["edges"]:
                outs[a].append((b[0]-a[0], b[1]-a[1]))
                ins[b].append((a[0]-b[0], a[1]-b[1]))
            for role in ("source", "target"):
                loc = self.location(state, f"{nid}:{role}")
                if loc:
                    c, arm = loc
                    (ins if role == "source" else outs)[c].append(arm)
                    protected[nid].add(c)
            incoming[nid], outgoing[nid] = ins, outs
        xs, ys = zip(*occupied)
        return bodies, occupied, incoming, outgoing, protected, (min(xs), min(ys), max(xs), max(ys))

    def mechanical(self, state):
        occupied, ports = set(), set()
        for fid, pose in state["placements"].items():
            if pose[2] not in self.catalog.facility_type(fid)["allowed_rotations"]:
                raise Mechanical("Disallowed facility rotation")
            body = self.body(fid, pose)
            if body & occupied:
                raise Mechanical("Facility collision")
            occupied.update(body)
        for nid, n in state["networks"].items():
            if occupied & n["cells"]:
                raise Mechanical("Transport collision or unsupported crossing")
            occupied.update(n["cells"])
            for a, b in n["edges"]:
                if a not in n["cells"] or b not in n["cells"] or abs(a[0]-b[0])+abs(a[1]-b[1]) != 1:
                    raise Mechanical("Invalid edge")
        for tid, binding in state["bindings"].items():
            if binding is None:
                continue
            nid, role, endpoint = self.catalog.terminal(tid)
            if "facility" in endpoint:
                fid = endpoint["facility"]
                port = next(p for p in self.catalog.facility_type(fid)["ports"] if p["id"] == binding["port"])
                if port["transport"] != "belt" or port["direction"] != ("output" if role == "source" else "input"):
                    raise Mechanical("Wrong terminal port role")
                key = fid, port["id"]
                if key in ports:
                    raise Mechanical("Port already bound")
                ports.add(key)
            c, _ = self.location(state, tid)
            if c not in state["networks"][nid]["cells"]:
                raise Mechanical("Bound terminal cell is absent")
        _, _, ins, outs, _, _ = self.index(state)
        for nid, n in state["networks"].items():
            for c in n["cells"]:
                i, o = ins[nid][c], outs[nid][c]
                if len(i) > 1 or len(o) > 1 or (i and o and i[0] == o[0]):
                    raise Mechanical("Unsupported junction or conflicting directed arms")

    def apply(self, state, command):
        op = command["op"]
        if op == "DISCARD_EDIT":
            return clone(self.base)
        result = clone(state)
        if op == "SET_PLACEMENTS":
            assignments = command["assignments"]
            if not 1 <= len(assignments) <= 2:
                raise Mechanical("Placement group limit is two")
            for fid, pose in assignments.items():
                if fid not in self.fids:
                    raise Mechanical("Unknown facility")
                if any(self.catalog.terminal(tid)[2].get("facility") == fid and b is not None
                       for tid, b in result["bindings"].items()):
                    raise Mechanical("Unbind a facility before moving it")
                if len(pose) != 3 or not all(type(v) is int for v in pose):
                    raise Mechanical("Placement coordinates must be integers")
                result["placements"][fid] = list(pose)
        elif op == "UNBIND_TERMINALS":
            tid = command["terminal"]
            if result["bindings"][tid] is None:
                raise Mechanical("Terminal is already unbound")
            result["bindings"][tid] = None
        elif op == "BIND_TERMINALS":
            tid = command["terminal"]
            if result["bindings"][tid] is not None:
                raise Mechanical("Unbind a terminal before rebinding")
            result["bindings"][tid] = dict(command["binding"])
            nid = self.catalog.terminal(tid)[0]
            result["networks"][nid]["cells"].add(self.location(result, tid)[0])
        elif op == "ERASE_TRANSPORT":
            nid = command["network"]
            cells = {tuple(c) for c in command["cells"]}
            if not 1 <= len(cells) <= 2 or not cells <= result["networks"][nid]["cells"]:
                raise Mechanical("Unknown cells or erase length outside domain")
            if len(cells) == 2:
                a, b = tuple(cells)
                if abs(a[0]-b[0])+abs(a[1]-b[1]) != 1:
                    raise Mechanical("Erase cells must be adjacent")
            result["networks"][nid]["cells"] -= cells
            result["networks"][nid]["edges"] = {(a, b) for a, b in result["networks"][nid]["edges"] if a not in cells and b not in cells}
        elif op == "EXTEND":
            nid, c = command["network"], tuple(command["anchor"])
            dx, dy = DIRECTIONS[command["direction"]]
            length = command["length"]
            if length not in self.config["segment_lengths"] or c not in result["networks"][nid]["cells"]:
                raise Mechanical("Invalid extension anchor or length")
            path = [(c[0]+dx*i, c[1]+dy*i) for i in range(length+1)]
            if any(p in result["networks"][nid]["cells"] for p in path[1:]):
                raise Mechanical("Extension cannot silently connect existing cells")
            result["networks"][nid]["cells"].update(path[1:])
            pairs = list(zip(path, path[1:]))
            if command["growth"] == "backward":
                pairs = [(b, a) for a, b in pairs]
            elif command["growth"] != "forward":
                raise Mechanical("Invalid growth direction")
            result["networks"][nid]["edges"].update(pairs)
        elif op == "CONNECT":
            nid = command["network"]
            edge = tuple(command["source"]), tuple(command["target"])
            if edge in result["networks"][nid]["edges"]:
                raise Mechanical("Connection already exists")
            result["networks"][nid]["edges"].add(edge)
        else:
            raise Mechanical(f"Unknown operation {op}")
        self.mechanical(result)
        return result

    def step(self, command):
        if self.steps >= self.config["horizon"]:
            raise Mechanical("Episode has ended; reset before another command")
        started = time.perf_counter()
        before_area = self.best_score[0]
        before = self.draft
        self.steps += 1
        self.episode_counts["commands"] += 1
        self.episode_counts[command.get("op", "unknown")] += 1
        outcome = {"applied": False, "complete": False, "new_completion": False, "improved": False}
        try:
            if command.get("revision", self.revision) != self.revision:
                raise Mechanical("Stale observation revision")
            self.draft = self.apply(self.draft, command)
            outcome["applied"] = True
            validation_started = time.perf_counter()
            accepted = check(self.catalog, serialize(self.draft))
            self.seconds["validation"] += time.perf_counter()-validation_started
            outcome["diagnostics"] = accepted["diagnostics"]
            if accepted["valid"]:
                outcome["complete"] = True
                outcome["new_completion"] = self.draft != before
                self.base, self.base_score = clone(self.draft), accepted["score"]
                if accepted["score"] < self.best_score:
                    self.best, self.best_score = clone(self.draft), accepted["score"]
                    outcome["improved"] = True
            elif accepted["diagnostics"]:
                self.episode_counts[accepted["diagnostics"][0]["code"]] += 1
        except (Mechanical, KeyError, ValueError, TypeError, StopIteration) as e:
            self.draft = before
            outcome["diagnostics"] = [{"code": "edit.rejected", "severity": "warning", "stage": "edit",
                                       "message": str(e), "entity": command.get("terminal", command.get("network"))}]
        self.revision += 1
        for key in ("applied", "complete", "new_completion", "improved"):
            self.episode_counts[key] += int(outcome[key])
        reward = (before_area-self.best_score[0])/self.initial_score[0]
        self.episode_counts["positive_rewards"] += int(reward > 0)
        self.trace.append({"command": deepcopy(command), "reward": reward, **outcome})
        self.seconds["step"] += time.perf_counter()-started
        return reward, self.steps == self.config["horizon"], outcome

    def candidates(self):
        started = time.perf_counter()
        s = self.draft
        bodies, occupied, ins, outs, protected, used = self.index(s)
        candidates = [{"op": "DISCARD_EDIT"}]
        port_use = {(self.catalog.terminal(tid)[2].get("facility"), b.get("port"))
                    for tid, b in s["bindings"].items() if b is not None and "port" in b}
        unbound_facilities = []
        transport = set().union(*(n["cells"] for n in s["networks"].values()))
        for fid in self.fids:
            if any(endpoint.get("facility") == fid and s["bindings"][tid] is not None
                   for tid in self.tids for _, _, endpoint in [self.catalog.terminal(tid)]):
                continue
            unbound_facilities.append(fid)
            x, y, r = s["placements"][fid]
            poses = {(x+dx*k, y+dy*k, r) for dx, dy in DIRECTIONS.values() for k in self.config["move_lengths"]}
            poses.update((x, y, rot) for rot in self.catalog.facility_type(fid)["allowed_rotations"] if rot != r)
            blocked = occupied - bodies[fid]
            for pose in sorted(poses):
                if not self.body(fid, pose) & blocked:
                    candidates.append({"op": "SET_PLACEMENTS", "assignments": {fid: list(pose)}})
        for i, a in enumerate(unbound_facilities):
            for b in unbound_facilities[i+1:]:
                pa, pb = s["placements"][a], s["placements"][b]
                aa, bb = [*pb[:2], pa[2]], [*pa[:2], pb[2]]
                body_a, body_b = self.body(a, aa), self.body(b, bb)
                blocked = occupied - bodies[a] - bodies[b]
                if not body_a & body_b and not (body_a | body_b) & blocked:
                    candidates.append({"op": "SET_PLACEMENTS", "assignments": {a: aa, b: bb}})
        for tid in self.tids:
            nid, role, endpoint = self.catalog.terminal(tid)
            if s["bindings"][tid] is not None:
                candidates.append({"op": "UNBIND_TERMINALS", "terminal": tid})
                continue
            options = []
            if "facility" in endpoint:
                fid = endpoint["facility"]
                for p in self.catalog.facility_type(fid)["ports"]:
                    if p["direction"] == ("output" if role == "source" else "input") and p["transport"] == "belt" and (fid, p["id"]) not in port_use:
                        options.append({"port": p["id"]})
            else:
                for c in sorted(s["networks"][nid]["cells"]):
                    for side, matches in zip(("W", "N", "E", "S"), (c[0] == used[0], c[1] == used[1], c[0] == used[2], c[1] == used[3])):
                        if matches:
                            options.append({"cell": list(c), "side": side})
            for binding in options:
                c, arm = self.location(s, tid, binding)
                if c in occupied and c not in s["networks"][nid]["cells"]:
                    continue
                own, other = (ins[nid][c], outs[nid][c]) if role == "source" else (outs[nid][c], ins[nid][c])
                if own or (other and other[0] == arm):
                    continue
                candidates.append({"op": "BIND_TERMINALS", "terminal": tid, "binding": binding})
        for nid, n in s["networks"].items():
            removable = n["cells"] - protected[nid]
            for c in sorted(removable):
                candidates.append({"op": "ERASE_TRANSPORT", "network": nid, "cells": [list(c)]})
            for a, b in sorted(n["edges"]):
                if a in removable and b in removable:
                    candidates.append({"op": "ERASE_TRANSPORT", "network": nid, "cells": [list(a), list(b)]})
            for c in sorted(n["cells"]):
                for growth, own, other in (("forward", outs[nid][c], ins[nid][c]), ("backward", ins[nid][c], outs[nid][c])):
                    if own:
                        continue
                    for direction, (dx, dy) in DIRECTIONS.items():
                        if other and other[0] == (dx, dy):
                            continue
                        for length in self.config["segment_lengths"]:
                            path = [(c[0]+dx*k, c[1]+dy*k) for k in range(1, length+1)]
                            if any(p in occupied for p in path):
                                continue
                            candidates.append({"op": "EXTEND", "network": nid, "anchor": list(c),
                                               "growth": growth, "direction": direction, "length": length})
                if not outs[nid][c]:
                    for dx, dy in DIRECTIONS.values():
                        d = c[0]+dx, c[1]+dy
                        if d in n["cells"] and not ins[nid][d] and (c, d) not in n["edges"]:
                            if ins[nid][c] and ins[nid][c][0] == (dx, dy):
                                continue
                            if outs[nid][d] and outs[nid][d][0] == (-dx, -dy):
                                continue
                            candidates.append({"op": "CONNECT", "network": nid, "source": list(c), "target": list(d)})
        candidates.sort(key=lambda c: json.dumps(c, sort_keys=True))
        self.counters["candidate_commands"] += len(candidates)
        self.seconds["candidates"] += time.perf_counter()-started
        return candidates

    def points(self, command):
        op = command["op"]
        if op == "SET_PLACEMENTS":
            points = [tuple(v[:2]) for v in command["assignments"].values()]
            return points[0], points[-1]
        if op in ("UNBIND_TERMINALS", "BIND_TERMINALS"):
            c = self.location(self.draft, command["terminal"], command.get("binding"))[0]
            return c, c
        if op == "ERASE_TRANSPORT":
            return tuple(command["cells"][0]), tuple(command["cells"][-1])
        if op == "EXTEND":
            c = tuple(command["anchor"])
            dx, dy = DIRECTIONS[command["direction"]]
            return c, (c[0]+dx*command["length"], c[1]+dy*command["length"])
        if op == "CONNECT":
            return tuple(command["source"]), tuple(command["target"])
        return tuple(self.draft["placements"][self.fids[0]][:2]), tuple(self.draft["placements"][self.fids[0]][:2])

    def observe(self, candidates):
        started = time.perf_counter()
        indexes = [self.index(s) for s in (self.draft, self.base)]
        all_points = indexes[0][1] | indexes[1][1]
        xs, ys = zip(*all_points)
        halo = self.config["halo"]
        x0, y0, x1, y1 = min(xs)-halo, min(ys)-halo, max(xs)+halo, max(ys)+halo
        width, height = x1-x0+1, y1-y0+1
        if width*height > self.config["raster_cell_guard"]:
            raise SpatialGuard(f"observation.resource_guard: {width}x{height} exceeds declared raster-work guard")
        per_state = len(self.fids)+len(self.nids)+8+len(self.tids)
        raster = np.zeros((2*per_state+3, height, width), dtype=np.float32)
        objects = []
        for state_index, (s, index) in enumerate(zip((self.draft, self.base), indexes)):
            offset = state_index*per_state
            bodies, _, ins, outs, _, _ = index
            for fi, fid in enumerate(self.fids):
                for x, y in bodies[fid]:
                    raster[offset+fi, y-y0, x-x0] = 1
            net_offset = offset+len(self.fids)
            arm_offset = net_offset+len(self.nids)
            terminal_offset = arm_offset+8
            for ni, nid in enumerate(self.nids):
                for x, y in s["networks"][nid]["cells"]:
                    raster[net_offset+ni, y-y0, x-x0] = 1
                    for io, arms in enumerate((ins[nid][x, y], outs[nid][x, y])):
                        for arm in arms:
                            raster[arm_offset+io*4+list(DIRECTIONS.values()).index(arm), y-y0, x-x0] = 1
            for ti, tid in enumerate(self.tids):
                nid, role, endpoint = self.catalog.terminal(tid)
                binding = s["bindings"][tid]
                loc = self.location(s, tid)
                position = loc[0] if loc else (0, 0)
                direction = list(DIRECTIONS.values()).index(loc[1]) if loc else -1
                if loc:
                    raster[terminal_offset+ti, position[1]-y0, position[0]-x0] = 1
                fid = endpoint.get("facility")
                pose = s["placements"][fid] if fid else [0, 0, 0]
                ft = self.catalog.facility_type(fid) if fid else None
                row = ([float(nid == n) for n in self.nids] + [float(fid == f) for f in self.fids]
                       + [float("external" in endpoint), float(role == "source"), float(binding is not None)]
                       + [position[0]/self.scale, position[1]/self.scale]
                       + [float(direction == i) for i in range(4)]
                       + [float(self.catalog.rate(nid)/self.catalog.capacity), float(self.catalog.capacity)]
                       + ([ft["footprint"]["width"]/self.scale, ft["footprint"]["height"]/self.scale] if ft else [0, 0])
                       + [pose[0]/self.scale, pose[1]/self.scale]
                       + [float(fid is not None and pose[2] == r) for r in (0, 90, 180, 270)])
                objects.append(row)
        raster[-3] = np.arange(x0, x1+1, dtype=np.float32)[None, :]/self.scale
        raster[-2] = np.arange(y0, y1+1, dtype=np.float32)[:, None]/self.scale
        raster[-1] = 1
        features, points = [], []
        for command in candidates:
            op = command["op"]
            assignments = command.get("assignments", {})
            selected = list(assignments)
            tid = command.get("terminal")
            nid = command.get("network", self.catalog.terminal(tid)[0] if tid else None)
            a, b = self.points(command)
            rotation = assignments[selected[0]][2] if selected else -1
            row = ([float(op == o) for o in OPERATIONS]
                   + [float(bool(selected) and selected[0] == f) for f in self.fids]
                   + [float(len(selected) > 1 and selected[1] == f) for f in self.fids]
                   + [float(nid == n) for n in self.nids] + [float(tid == t) for t in self.tids]
                   + [float(command.get("direction", command.get("binding", {}).get("side")) == d) for d in SIDES]
                   + [float(command.get("growth") == g) for g in ("forward", "backward")]
                   + [float(rotation == r) for r in (0, 90, 180, 270)]
                   + [a[0]/self.scale, a[1]/self.scale, b[0]/self.scale, b[1]/self.scale,
                      (b[0]-a[0])/self.scale, (b[1]-a[1])/self.scale,
                      command.get("length", len(command.get("cells", [])))/2, len(assignments)/2])
            features.append(row)
            points.append([a[0]-x0, a[1]-y0, b[0]-x0, b[1]-y0])
        stats = [(self.config["horizon"]-self.steps)/self.config["horizon"],
                 self.best_score[0]/self.initial_score[0], self.base_score[0]/self.initial_score[0],
                 self.best_score[1]/self.initial_score[0], self.base_score[1]/self.initial_score[0],
                 self.best_score[2]/self.initial_score[0], self.base_score[2]/self.initial_score[0],
                 sum(b is None for b in self.draft["bindings"].values())/len(self.tids)]
        observation = {"raster": raster, "objects": np.asarray(objects, dtype=np.float32),
                       "stats": np.asarray(stats, dtype=np.float32),
                       "candidates": np.asarray(features, dtype=np.float32), "points": np.asarray(points, dtype=np.int64)}
        self.seconds["observation"] += time.perf_counter()-started
        return observation
