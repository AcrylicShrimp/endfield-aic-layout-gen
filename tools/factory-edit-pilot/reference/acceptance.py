"""Independent serialized-layout acceptance for the declared directed-path scope."""

from collections import Counter, defaultdict
from fractions import Fraction
import json
from pathlib import Path


DIRECTIONS = {"N": (0, -1), "E": (1, 0), "S": (0, 1), "W": (-1, 0)}
SIDES = tuple(DIRECTIONS)


class Invalid(ValueError):
    def __init__(self, code, message, entity=None):
        self.diagnostic = {"code": code, "severity": "error", "stage": "acceptance",
                           "message": message, "entity": entity}
        super().__init__(message)


def require(condition, code, message, entity=None):
    if not condition:
        raise Invalid(code, message, entity)


class Catalog:
    def __init__(self, root, case_path):
        self.root = Path(root)
        self.path = Path(case_path)
        self.case = json.loads(self.path.read_text())
        require(self.case["schema_version"] == 1, "input.schema", "Unsupported case schema", str(case_path))
        self.paths = {k: self.root / v for k, v in self.case["catalogs"].items()}
        data = {k: json.loads(v.read_text()) for k, v in self.paths.items()}
        self.recipes = {x["id"]: x for x in data["recipes"]["recipes"]}
        self.types = {x["id"]: x for x in data["facilities"]["facilities"]}
        self.external = set(data["recipes"]["external_items"])
        self.facilities = {x["id"]: x for x in self.case["facilities"]}
        self.networks = {x["id"]: x for x in self.case["networks"]}
        self.terminal_ids = [f"{n}:{role}" for n in self.networks for role in ("source", "target")]
        belt = next(x for x in data["transports"]["transports"] if x["kind"] == "belt")
        self.capacity = Fraction(belt["capacity"]["quantity"] * 1000, belt["capacity"]["duration_ms"])
        self.check_request()

    def facility_type(self, fid):
        return self.types[self.recipes[self.facilities[fid]["recipe"]]["facility"]]

    def terminal(self, tid):
        nid, role = tid.split(":")
        return nid, role, self.networks[nid][role]

    def rate(self, nid):
        return Fraction(*self.networks[nid]["rate"])

    def check_request(self):
        require(len(self.facilities) == len(self.case["facilities"]), "input.duplicate", "Duplicate facilities")
        require(len(self.networks) == len(self.case["networks"]), "input.duplicate", "Duplicate networks")
        expected, observed = defaultdict(Fraction), defaultdict(Fraction)
        for fid, f in self.facilities.items():
            r = self.recipes[f["recipe"]]
            utilization = Fraction(*f["utilization"])
            require(0 < utilization <= 1, "request.utilization", "Utilization outside (0,1]", fid)
            for role, entries in (("target", r["inputs"]), ("source", r["outputs"])):
                for entry in entries:
                    expected[fid, role, entry["item"]] += utilization * Fraction(entry["quantity"] * 1000, r["duration_ms"])
        exports = defaultdict(Fraction)
        for nid, n in self.networks.items():
            rate = self.rate(nid)
            require(0 < rate <= self.capacity, "request.capacity", "Network exceeds catalog belt capacity", nid)
            for role in ("source", "target"):
                endpoint = n[role]
                require(set(endpoint) in ({"facility"}, {"external"}), "request.endpoint", "Invalid endpoint", nid)
                if "facility" in endpoint:
                    require(endpoint["facility"] in self.facilities, "request.facility", "Unknown facility", nid)
                    observed[endpoint["facility"], role, n["item"]] += rate
                elif role == "source":
                    require(n["item"] in self.external, "request.external", "Undeclared external input item", nid)
                else:
                    exports[n["item"]] += rate
        require(dict(expected) == dict(observed), "request.coverage", "Networks do not exactly cover recipe obligations")
        target = self.case["target"]
        require(dict(exports) == {target["item"]: Fraction(*target["rate"])}, "request.target", "External output differs from target")


def footprint(catalog, fid, pose):
    x, y, rotation = pose
    definition = catalog.facility_type(fid)
    require(rotation in definition["allowed_rotations"], "geometry.rotation", "Disallowed rotation", fid)
    width, height = definition["footprint"]["width"], definition["footprint"]["height"]
    if rotation % 180:
        width, height = height, width
    return {(x + dx, y + dy) for dx in range(width) for dy in range(height)}


def port_geometry(catalog, fid, pose, port_id):
    definition = catalog.facility_type(fid)
    ports = {p["id"]: p for p in definition["ports"]}
    require(port_id in ports, "terminal.port", "Unknown catalog port", f"{fid}/{port_id}")
    p = ports[port_id]
    x, y, rotation = pose
    a, b = p["position"]["x"], p["position"]["y"]
    width, height = definition["footprint"]["width"], definition["footprint"]["height"]
    for _ in range(rotation // 90):
        a, b, width, height = height - 1 - b, a, height, width
    side = SIDES[(SIDES.index(p["edge"][0].upper()) + rotation // 90) % 4]
    dx, dy = DIRECTIONS[side]
    return (x + a + dx, y + b + dy), (-dx, -dy), p


def bounds(points):
    require(bool(points), "geometry.empty", "Empty used geometry")
    xs, ys = zip(*points)
    return min(xs), min(ys), max(xs), max(ys)


def on_boundary(cell, side, used):
    x, y = cell
    x0, y0, x1, y1 = used
    return {"N": y == y0, "S": y == y1, "W": x == x0, "E": x == x1}.get(side, False)


def check(catalog, layout):
    """Rebuild acceptance from serialized DTOs, never from environment caches."""
    try:
        require(layout.get("schema_version") == 1, "layout.schema", "Unsupported layout schema")
        poses = layout["placements"]
        require(set(poses) == set(catalog.facilities), "layout.inventory", "Facility inventory changed")
        require(set(layout["networks"]) == set(catalog.networks), "layout.networks", "Network inventory changed")
        occupied = set()
        for fid, pose in poses.items():
            require(len(pose) == 3 and all(type(v) is int for v in pose), "geometry.integer", "Invalid integer pose", fid)
            body = footprint(catalog, fid, pose)
            require(not body & occupied, "geometry.collision", "Facility footprints overlap", fid)
            occupied.update(body)
        bindings = layout["bindings"]
        require(set(bindings) == set(catalog.terminal_ids), "terminal.inventory", "Terminal inventory changed")
        all_cells = set(occupied)
        prepared = {}
        for nid, n in layout["networks"].items():
            cells = {tuple(c) for c in n["cells"]}
            require(len(cells) == len(n["cells"]) and bool(cells), "network.cells", "Empty or duplicate network cells", nid)
            require(all(len(c) == 2 and all(type(v) is int for v in c) for c in cells), "geometry.integer", "Noninteger network cell", nid)
            require(not cells & all_cells, "geometry.collision", "Transport overlap or unsupported crossing", nid)
            all_cells.update(cells)
            edges = [(tuple(a), tuple(b)) for a, b in n["edges"]]
            require(len(set(edges)) == len(edges), "network.edge", "Duplicate directed edge", nid)
            prepared[nid] = cells, edges
        used = bounds(all_cells)
        port_use = set()
        turns, tile_count = 0, 0
        for nid, (cells, edges) in prepared.items():
            incoming, outgoing = defaultdict(list), defaultdict(list)
            adjacency = defaultdict(list)
            for a, b in edges:
                require(a in cells and b in cells and abs(a[0]-b[0])+abs(a[1]-b[1]) == 1,
                        "network.edge", "Edge is not an occupied adjacent cell pair", nid)
                outgoing[a].append((b[0]-a[0], b[1]-a[1]))
                incoming[b].append((a[0]-b[0], a[1]-b[1]))
                adjacency[a].append(b)
            terminal_cells = {}
            for role in ("source", "target"):
                tid = f"{nid}:{role}"
                binding = bindings[tid]
                require(binding is not None, "terminal.unbound", "Unbound required terminal", tid)
                endpoint = catalog.networks[nid][role]
                if "facility" in endpoint:
                    fid = endpoint["facility"]
                    require(set(binding) == {"port"}, "terminal.binding", "Expected a facility port binding", tid)
                    cell, arm, p = port_geometry(catalog, fid, poses[fid], binding["port"])
                    expected_direction = "output" if role == "source" else "input"
                    require(p["direction"] == expected_direction and p["transport"] == "belt", "terminal.direction", "Wrong port direction or transport", tid)
                    key = fid, binding["port"]
                    require(key not in port_use, "terminal.port_reuse", "Physical port assigned more than once", tid)
                    port_use.add(key)
                else:
                    require(set(binding) == {"cell", "side"}, "terminal.binding", "Expected external boundary binding", tid)
                    cell = tuple(binding["cell"])
                    require(binding["side"] in DIRECTIONS and on_boundary(cell, binding["side"], used),
                            "terminal.boundary", "External endpoint is not on actual used boundary", tid)
                    arm = DIRECTIONS[binding["side"]]
                require(cell in cells, "terminal.cell", "Bound terminal has no network cell", tid)
                terminal_cells[role] = cell
                (incoming if role == "source" else outgoing)[cell].append(arm)
            for c in cells:
                require(len(incoming[c]) == len(outgoing[c]) == 1, "network.degree", "Incomplete path or unsupported junction", nid)
                require(incoming[c][0] != outgoing[c][0], "network.arm", "Input and output use the same side", nid)
                turns += incoming[c][0] != (-outgoing[c][0][0], -outgoing[c][0][1])
            visited = set()
            frontier = [terminal_cells["source"]]
            while frontier:
                c = frontier.pop()
                if c not in visited:
                    visited.add(c)
                    frontier.extend(adjacency[c])
            require(visited == cells and terminal_cells["target"] in visited,
                    "network.reachability", "Disconnected geometry or transport cycle", nid)
            require(catalog.rate(nid) <= catalog.capacity, "network.capacity", "Path exceeds belt capacity", nid)
            tile_count += len(cells)
        score = [(used[2]-used[0]+1)*(used[3]-used[1]+1), tile_count, turns]
        return {"valid": True, "score": score, "bounds": list(used), "diagnostics": []}
    except Invalid as e:
        return {"valid": False, "score": None, "diagnostics": [e.diagnostic]}
    except (KeyError, ValueError, TypeError, IndexError) as e:
        return {"valid": False, "score": None, "diagnostics": [{"code": "layout.malformed", "severity": "error",
                "stage": "acceptance", "message": str(e), "entity": None}]}


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", type=Path, required=True)
    parser.add_argument("--layout", type=Path, required=True)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    args = parser.parse_args()
    catalog = Catalog(args.root, args.case)
    result = check(catalog, json.loads(args.layout.read_text()))
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result["valid"] else 1)


if __name__ == "__main__":
    main()
