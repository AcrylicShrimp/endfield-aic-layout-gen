# Dense Reusable A* Workspace

## Question

The six-step automatic assembly benchmark spends increasing time while attaching immutable process modules. This experiment identifies the hot path and tests a semantics-preserving routing-engine reformulation before restricting placement candidates.

## Profile

A 15-second release-mode sample of the four-step run identified repeated A* routing as the dominant active code. The collapsed top-of-stack sample contained:

- 8,759 samples in `route_shortest_path`;
- 8,824 samples in SipHash used by routing-state hash maps;
- 4,060 additional routing-state hash-builder samples;
- only 169 samples directly in the surrounding composition loop.

Selected winning compositions alone issued between 902 and 33,029 A* searches per step. The previous A* implementation allocated two hash maps for every search and hashed `(x, y, heading)` states despite operating on a small bounded grid.

## Change

Routing state is now indexed directly as `(cell_index * 4 + heading)`. One `RouteWorkspace` owns dense best-cost, predecessor, generation, and heap storage for a composition candidate. Generation stamps reset logical state between searches without clearing the dense arrays, and the heap retains its allocation.

The public constructive behavior is unchanged:

- A* still minimizes path steps first and turns second;
- blocked cells and canvas bounds are unchanged;
- every placement, rotation, and port pair is still evaluated;
- automatic candidate ranking is unchanged.

A deterministic 200-grid test compares the dense implementation against the previous hash-map formulation for feasibility, shortest-path length, and minimum turns.

## Result

| Metric | Hash-map baseline | Dense workspace | Change |
| --- | ---: | ---: | ---: |
| Six-step wall time | 61.49 s | 26.72 s | 2.30x faster |
| Step 1 | 0.667 s | 0.415 s | 1.61x faster |
| Step 2 | 1.852 s | 1.190 s | 1.56x faster |
| Step 3 | 8.168 s | 3.549 s | 2.30x faster |
| Step 4 | 14.943 s | 7.075 s | 2.11x faster |
| Step 5 | 15.940 s | 6.359 s | 2.51x faster |
| Step 6 | 19.029 s | 7.827 s | 2.43x faster |

After removing elapsed-time fields, the complete machine-readable reports are identical. The selected modules, placements, ports, routes, scores, bounds, and unresolved frontier did not change. The final partial factory remains 15 facilities and 14 routed requirements in `53x11`.

## Additive Placement Interpretation

Composition already keeps the entire existing target node immutable. Only the new source module receives a rigid rotation and translation, after which one boundary route is added. The current cost is therefore not caused by moving old facilities.

The remaining waste is that all legal rigid placements and port pairs are routed before the best attachment is known. A next exact-equivalent optimization can evaluate compact adjacent placements first to obtain an incumbent, then reject a placement without A* when its facility-and-existing-route bounding-box lower bound is already worse than that incumbent. This uses additive placement as a search order and bound, not as a candidate restriction.

## Artifacts

- `report.json`: six-step automatic assembly report.
- `heavy-xiranite.html`: localized six-page layout history.
