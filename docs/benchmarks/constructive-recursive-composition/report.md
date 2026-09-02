# Recursive Constructive Composition

## Purpose

This slice tests whether the same small placement-and-routing operation can treat an existing process module as one immutable node. It composes:

- the previously constructed three-facility Xiranite-powder module;
- the final Xiranite oven as a one-facility leaf node;
- the Xiranite-powder belt requirement between their exposed boundaries.

The operation is generic over two `ConstructiveNode` values. The process-module CLI wrapper only builds the two inputs and selects the logical edge.

## Composition Contract

The target node remains fixed while the source node may move and rotate as one unit. Every facility, routed cell, terminal, and still-open boundary port inside that source node receives the same rigid transform. Internal module geometry is never rebuilt during composition.

Candidate validation uses actual occupied cells rather than the full rectangular module bounds:

- facility footprints cannot overlap other facilities or either transport layer;
- same-layer transport cells cannot overlap;
- belt and pipe may share a world coordinate;
- the selected boundary ports must remain reachable;
- every unconnected logical boundary must retain at least one physically usable port option.

Candidate comparison first minimizes lost boundary-port options, then used bounding-box area, occupied transport tiles, and route turns. This makes future interface survival the construction guard while the reported factory score remains based on actual used geometry.

## Heavy Xiranite Result

- Status: constructed
- Release wall time: 0.61 seconds
- Facilities: 4
- Routed internal requirements: 3
- Used bounds: `14x8` (area 112)
- Occupied belt tiles: 9
- Occupied pipe tiles: 0
- Route turns: 0
- Remaining boundary requirements: 6
- Remaining boundary port options: 16
- Boundary port options removed by this composition: 1

The two enriched-carbon routes inside the source module remain unchanged. Composition adds one straight five-tile Xiranite-powder belt from the module oven to the final oven. The result preserves the other unresolved inputs and outputs as boundary option domains.

## Search Evidence

- Whole-node rotations considered: 4
- Whole-node placements considered: 776
- Colliding placements rejected: 238
- Boundary port pairs considered: 13,450
- Blocked port pairs rejected: 496
- A* searches: 12,954
- A* failures: 0
- Candidates rejected because a future boundary became unusable: 950
- Valid candidates scored: 12,004

This search is local to two already-valid nodes. It does not revisit the three-facility module's internal placement, port assignments, or routes. The run therefore demonstrates recursive reuse of the same macro placement-and-routing contract without replaying the global chronological prefix.

## Scope

This is the first recursive composition, not automatic full-graph decomposition. It joins one module and one facility through one requirement. The remaining same-item Xiranite-powder input demonstrates the next required capability: construct or clone another compatible supplier node, then compose it into the current composite through the same operation.

The result is heuristic and does not prove a global or local optimum. The fixed target node is an intentional direction for this constructive step, and later iterative improvement may move or rebuild a bounded composite region.

## Artifacts

- `report.json`: machine-readable composite node, open boundaries, score, and candidate statistics.
- `heavy-xiranite-powder-to-final.html`: interactive localized wireframe with routed cells and clickable open boundary ports.
