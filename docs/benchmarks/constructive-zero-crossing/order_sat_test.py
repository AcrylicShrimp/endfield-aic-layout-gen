"""Local mathematical fixtures; no production or runtime catalog replacement."""
import unittest
import itertools

from order_sat import solve


def fixture(pairs, ports=2):
    names = sorted({n for pair in pairs for n in pair})
    placements = [{"instance": n, "block": n, "catalog_ports": [
        {"id": f"{direction}-{i}", "transport": "belt", "direction": direction,
         "edge": face, "connection": {"x": 0, "y": i}}
        for direction, face in [("input", "west"), ("output", "east")] for i in range(ports)]}
        for n in names]
    return {"corridors": [{"id": "fixture", "status": "locally-certified", "edges": [
        {"id": str(i), "source": a, "target": b, "rate": {"numerator": 1, "denominator": 4}}
        for i, (a, b) in enumerate(pairs)]}],
        "composition": {"candidate": {"placements": placements, "block_experiment": {"blocks": [
            {"id": n, "y": 0, "height": ports} for n in names]}}}}


TRANSPORTS = {"transports": [{"kind": "belt", "capacity": {"quantity": 1, "duration_ms": 2000}}]}


class OrderTests(unittest.TestCase):
    def test_reversed_edges_reorder_and_assign_real_ports(self):
        r = solve(fixture([("a", "y"), ("b", "x")]), TRANSPORTS, "all-forward")
        self.assertEqual(r["status"], "OPTIMAL")
        self.assertEqual(r["corridors"][0]["strict_order_violations"], 0)
        self.assertFalse(r["global_routing_allowed"])

    def test_k22_is_infeasible_only_for_independent_links(self):
        r = solve(fixture([(a, b) for a in ["a", "b"] for b in ["x", "y"]]), TRANSPORTS, "all-forward")
        self.assertEqual(r["status"], "INFEASIBLE")
        self.assertEqual(r["sufficient_corridor_assumption_core"], ["fixture"])
        self.assertFalse(r["factory_validated"])

    def test_port_shortage_is_not_hidden_by_same_commodity_sharing(self):
        r = solve(fixture([("a", "x"), ("a", "y")], ports=1), TRANSPORTS, "all-forward")
        self.assertEqual(r["status"], "INFEASIBLE")

    def test_two_lane_demand_is_preserved(self):
        p = fixture([("a", "x")])
        p["corridors"][0]["edges"][0]["rate"] = {"numerator": 1, "denominator": 1}
        r = solve(p, TRANSPORTS, "all-forward")
        self.assertEqual(len(r["corridors"][0]["lanes"]), 2)
        self.assertEqual(len({l["target_port"] for l in r["corridors"][0]["lanes"]}), 2)

    def test_small_graphs_match_independent_permutation_enumeration(self):
        universe = list(itertools.product(["a", "b", "c"], ["x", "y", "z"]))
        for mask in [1, 3, 13, 42, 85, 127, 170, 255, 341, 511]:
            pairs = [p for i, p in enumerate(universe) if mask & (1 << i)]
            sources = sorted({a for a, _ in pairs})
            targets = sorted({b for _, b in pairs})
            possible = any(all((s.index(a) - s.index(c)) * (t.index(b) - t.index(d)) >= 0
                               for (a, b), (c, d) in itertools.combinations(pairs, 2))
                           for s in itertools.permutations(sources) for t in itertools.permutations(targets))
            result = solve(fixture(pairs, ports=3), TRANSPORTS, "all-forward")
            self.assertEqual(result["status"] == "OPTIMAL", possible, mask)


if __name__ == "__main__":
    unittest.main()
