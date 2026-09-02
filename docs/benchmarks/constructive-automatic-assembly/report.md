# Automatic Constructive Module Assembly

## Purpose

This slice removes the explicit process-module list from the constructive assembly path. The caller supplies only the seed target facility and a growth-step budget. At each step the planner:

1. inspects the current composite's unresolved facility-supplied input boundaries;
2. finds the producer facility behind each boundary;
3. groups that producer with each distinct directly supplied facility input item;
4. constructs every resulting small process-module candidate;
5. attempts the same generic node composition for every valid candidate;
6. selects the best composable candidate by the established constructive score;
7. uses the selected composite as the next step's target node.

External inputs and final outputs are ignored by module discovery. No Heavy-Xiranite facility, item, edge, or module identifier appears in the discovery implementation.

## Heavy Xiranite Result

- Status: requested two automatic steps constructed
- Complete factory: no; the explicit two-step experiment limit was reached
- Release wall time: 5.28 seconds
- User-selected module roots: 0
- User-selected internal items: 0
- User-selected wiring requirements: 0
- Final facilities: 6
- Final routed internal requirements: 5
- Final bounds: `27x8` (area 216)
- Belt tiles: 23
- Pipe tiles: 12
- Total transport tiles: 35
- Route turns: 4
- Remaining facility-supplied requirements: 4

## Automatic Choices

| Step | Frontier edges | Candidates generated | Composable | Selected local process | Added facilities | Result |
| ---: | ---: | ---: | ---: | --- | ---: | --- |
| 1 | 3 | 3 | 3 | liquid-Xiranite-poly producer plus its liquid-Xiranite supplier | 2 | `12x9`, area 108 |
| 2 | 3 | 3 | 3 | Xiranite-powder producer plus two enriched-carbon suppliers | 3 | `27x8`, area 216 |

No candidate module construction or composition failed in either step. The planner did not reproduce the previous hand-written powder-first assembly order. It evaluated all three initial frontier candidates and selected the smaller mixed-pipe process first. At step two it followed that process upstream and automatically selected its powder-production block.

## Selected Composition Metrics

| Metric | Step 1 | Step 2 |
| --- | ---: | ---: |
| Whole-node placements considered | 712 | 1,564 |
| Colliding placements rejected | 256 | 683 |
| Boundary port pairs considered | 912 | 8,810 |
| A* searches | 902 | 8,652 |
| Future-boundary dead ends rejected | 286 | 263 |
| Valid candidates scored | 616 | 8,389 |
| Boundary options blocked by winner | 1 | 0 |
| Winner transport tiles | 12 | 35 |
| Winner route turns | 2 | 4 |

The 5.28-second wall time includes constructing and composing all three candidates at both steps, not only the two winners. This is the measured cost of removing the manual module plan. Candidate evaluation remains independent and is a future parallelization opportunity.

## Remaining Frontier

After two steps, four facility-supplied inputs remain:

- two enriched-carbon-powder belt inputs for the newly selected powder block;
- two Xiranite-powder belt inputs directly required by the final oven.

Every unresolved boundary retains physical port options. Increasing the step budget can continue discovery from this exact logical frontier without adding module identifiers to the request.

## Current Boundary Of Automation

This first discovery rule creates a module only when a frontier producer has at least one direct facility-supplied input item. A leaf producer with only external inputs and an SCC/cyclic neighborhood are not yet automatic module candidates. Shared upstream instance ownership is rejected rather than duplicated.

The next bottom-up experiment should increase the step budget until this rule reaches its first unsupported frontier. The result will determine whether leaf-node synthesis, SCC module synthesis, shared-subgraph ownership, or composition recovery is the next required feature.

## Artifacts

- `report.json`: discovery counts, selected candidates, two composition reports, and final partial composite.
- `heavy-xiranite.html`: localized two-page automatic growth history.
