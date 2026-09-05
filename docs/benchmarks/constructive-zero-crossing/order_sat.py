# /// script
# requires-python = ">=3.11"
# dependencies = ["ortools>=9,<10"]
# ///
"""Bounded local order/port oracle, deliberately excluding coordinates and path search."""
import argparse
import hashlib
import itertools
import json
import time
from fractions import Fraction
from pathlib import Path

import ortools
from ortools.sat.python import cp_model


def solve(report, transports, scope, corridor_id=None, seconds=5):
    start = time.perf_counter()
    all_corridors = report["corridors"]
    if scope == "all-forward":
        corridors = all_corridors
    elif scope == "orderable-slice":
        corridors = [c for c in all_corridors if c["status"] == "locally-certified"]
    else:
        corridors = [c for c in all_corridors if c["id"] == corridor_id]
    if not corridors:
        raise ValueError("No corridors selected by explicit diagnostic scope")
    placement = report["composition"]["candidate"]
    facilities = {p["instance"]: p for p in placement["placements"]}
    blocks = {b["id"]: b for b in placement["block_experiment"]["blocks"]}
    capacity = next(t["capacity"] for t in transports["transports"] if t["kind"] == "belt")
    rate_capacity = Fraction(capacity["quantity"] * 1000, capacity["duration_ms"])
    model = cp_model.CpModel()
    ranks = {b: model.new_int_var(0, len(blocks) - 1, b) for b in blocks}
    model.add_all_different(list(ranks.values()))
    stride = max(b["height"] for b in blocks.values()) + 1
    lane_groups, domains, ports_by_face, assumptions = [], {}, {}, {}
    for corridor in corridors:
        lanes = []
        for edge in corridor["edges"]:
            remaining = Fraction(edge["rate"]["numerator"], edge["rate"]["denominator"])
            lane_index = 0
            while remaining:
                amount = min(remaining, rate_capacity)
                remaining -= amount
                lane = {"requirement": edge["id"], "lane": lane_index,
                        "rate": {"numerator": amount.numerator, "denominator": amount.denominator},
                        "source": edge["source"], "target": edge["target"]}
                for role, direction, face in [("source", "output", "east"), ("target", "input", "west")]:
                    p = facilities[edge[role]]
                    candidates = [q for q in p["catalog_ports"] if q["transport"] == "belt"
                                  and q["direction"] == direction and q["edge"] == face]
                    if not candidates:
                        raise ValueError(f"No facing ports: {edge['id']} / {role}")
                    key = (edge["id"], lane_index, role)
                    index = model.new_int_var(0, len(candidates) - 1, f"port:{len(domains)}")
                    relative_rows = [q["connection"]["y"] - blocks[p["block"]]["y"] for q in candidates]
                    row = model.new_int_var(min(relative_rows), max(relative_rows), f"row:{len(domains)}")
                    model.add_element(index, relative_rows, row)
                    domains[key] = (index, candidates)
                    ports_by_face.setdefault((p["instance"], direction), []).append(index)
                    lane[role + "_order"] = ranks[p["block"]] * stride + row
                lanes.append(lane)
                lane_index += 1
                if lane_index > 1000:
                    raise ValueError("Local lane safety bound exceeded")
        active = model.new_bool_var(f"corridor:{corridor['id']}")
        model.add_assumption(active)
        assumptions[active.index] = corridor["id"]
        for a, b in itertools.combinations(lanes, 2):
            comparisons = []
            for role in ("source", "target"):
                before = model.new_bool_var(f"before:{len(model.proto.variables)}")
                model.add(a[role + "_order"] < b[role + "_order"]).only_enforce_if(before)
                model.add(a[role + "_order"] >= b[role + "_order"]).only_enforce_if(before.Not())
                comparisons.append(before)
            model.add(comparisons[0] == comparisons[1]).only_enforce_if(active)
        lane_groups.append((corridor["id"], lanes))
    for variables in ports_by_face.values():
        model.add_all_different(variables)
    build_ms = (time.perf_counter() - start) * 1000
    solver = cp_model.CpSolver()
    solver.parameters.max_time_in_seconds = seconds
    solver.parameters.num_search_workers = 1
    solver.parameters.random_seed = 0
    solver.parameters.log_search_progress = False
    status = solver.solve(model)
    result = {"schema_version": 1, "kind": "zero-crossing-local-order-oracle", "scope": scope,
              "placement_sha256": report.get("provenance", {}).get("inputs", {}).get("placement", {}).get("sha256"),
              "transports_sha256": report.get("provenance", {}).get("inputs", {}).get("transports", {}).get("sha256"),
              "selected_corridors": [c["id"] for c in corridors],
              "unmodeled_corridors": [c["id"] for c in all_corridors if c not in corridors],
              "global_routing_allowed": False, "factory_validated": False,
              "semantics": "Rigid block order and distinct facing ports for independent lanes in selected forward corridors only; no merge/split networks, return paths, pipes, coordinates or path embedding.",
              "ortools_version": ortools.__version__, "status": solver.status_name(status),
              "time_limit_seconds": seconds, "workers": 1,
              "variables": len(model.proto.variables), "constraints": len(model.proto.constraints),
              "build_ms": build_ms, "search_ms": solver.wall_time * 1000,
              "conflicts": solver.num_conflicts, "branches": solver.num_branches}
    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        result["block_order"] = sorted(ranks, key=lambda b: solver.value(ranks[b]))
        result["corridors"] = []
        for cid, lanes in lane_groups:
            assigned = []
            for lane in lanes:
                row = {k: v for k, v in lane.items() if not k.endswith("_order")}
                for role in ("source", "target"):
                    index, candidates = domains[(lane["requirement"], lane["lane"], role)]
                    row[role + "_port"] = candidates[solver.value(index)]["id"]
                    row[role + "_order"] = solver.value(lane[role + "_order"])
                assigned.append(row)
            violations = [1 for a, b in itertools.combinations(assigned, 2)
                          if (a["source_order"] - b["source_order"]) * (a["target_order"] - b["target_order"]) <= 0]
            if violations:
                raise AssertionError("Independent order validation rejected solver witness")
            result["corridors"].append({"id": cid, "lanes": assigned, "strict_order_violations": 0})
    elif status == cp_model.INFEASIBLE:
        result["sufficient_corridor_assumption_core"] = [assumptions[i] for i in solver.sufficient_assumptions_for_infeasibility()]
        result["proof_scope"] = "This selected independent-lane model only. The sufficient ordering-assumption core retains all background port constraints; it is not necessarily minimal and is not global factory infeasibility."
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--transports", required=True)
    parser.add_argument("--scope", required=True, choices=["all-forward", "orderable-slice", "corridor"])
    parser.add_argument("--corridor")
    parser.add_argument("--seconds", type=float, default=5)
    parser.add_argument("--report-output", required=True)
    args = parser.parse_args()
    if not 0 < args.seconds <= 30:
        parser.error("--seconds must be greater than zero and at most 30")
    output = Path(args.report_output)
    if output.exists():
        parser.error(f"Output already exists: {output}")
    report = json.loads(Path(args.input).read_text())
    data = Path(args.transports).read_bytes()
    if hashlib.sha256(data).hexdigest() != report["provenance"]["inputs"]["transports"]["sha256"]:
        parser.error("Transport catalog does not match corridor analysis")
    result = solve(report, json.loads(data), args.scope, args.corridor, args.seconds)
    with output.open("x") as file:
        json.dump(result, file, indent=2)
        file.write("\n")
    print(json.dumps({k: result[k] for k in ("scope", "status", "variables", "constraints", "build_ms", "search_ms")}))


if __name__ == "__main__":
    main()
