# Heavy Xiranite Minimum-Rate Phase 2 Rotation Partition Report

## Question

After fixing one unresolved coordinate and one unresolved complete port assignment of the Phase 2 introduced facility, which rotation remains possible and does a validated 12 by 12 witness exist?

## Fixed context

- Exact used dimensions: 12 by 12.
- Introduced facility coordinate: `(0, 1)`.
- Complete port assignment: index 5 from the exact 125-case port partition.
- Every legal rotation was tested.
- The other two facilities, their ports, all external boundary terminals, and all routing remained solver decisions.

## One-second result

| Rotation | Outcome | Search time |
| ---: | --- | ---: |
| 0 degrees | Proven infeasible | 34 ms |
| 90 degrees | Proven infeasible | 32 ms |
| 180 degrees | Unknown | 1,003 ms |
| 270 degrees | Proven infeasible | 34 ms |

The fixation propagated strongly: three rotations were rejected almost immediately and only 180 degrees survived the short budget.

## Thirty-second result

| Rotation | Outcome | Search time |
| ---: | --- | ---: |
| 0 degrees | Proven infeasible | 30 ms |
| 90 degrees | Proven infeasible | 29 ms |
| 180 degrees | Validated feasible | 15,582 ms |
| 270 degrees | Proven infeasible | 31 ms |

The 180-degree case produced the first validated cumulative Phase 2 witness.

## Witness

| Metric | Value |
| --- | ---: |
| Bounds | 12 by 12 |
| Facility count | 3 |
| Physical transport tiles | 103 |
| Route turns | 40 |
| Logistics components | 24 |
| First witness time | 15,582 ms |
| Validation | Passed |

Facility placements:

| Facility | X | Y | Rotation |
| --- | ---: | ---: | ---: |
| Mix pool | 1 | 7 | 0 degrees |
| Introduced xiranite oven | 0 | 1 | 180 degrees |
| Target xiranite oven | 7 | 1 | 0 degrees |

This establishes a Phase 2 area upper bound of 144. It does not prove the primary optimum because the 11 by 12 and 12 by 11 dimension cases remain unresolved.

## Cliff conclusion

The third facility's placement and port choices form an important branching layer, but they are not the final first-witness cliff. Even after fixing its coordinate, rotation, and all four logical port choices, Pumpkin required 15.6 seconds to choose the other two facility placements, their ports, eight external boundary terminals, and the shared belt/pipe routing witness.

The next diagnostic should reuse this validated witness as a diagnostic reference and compare:

1. all three facility placements fixed, terminals and routing free;
2. all facility placements and facility port choices fixed, external terminals and routing free;
3. all placements and every terminal fixed, routing free.

This is a diagnostic ablation only. It must not become a production fallback or restrict the authoritative solver solution set.

## Artifacts

- `heavy-xiranite-phase2-rotation-partition-12x12-x0-y1-ports5/summary.json`
- `heavy-xiranite-phase2-rotation-partition-12x12-x0-y1-ports5/summary.html`
- `heavy-xiranite-phase2-rotation-partition-12x12-x0-y1-ports5-30s/summary.json`
- `heavy-xiranite-phase2-rotation-partition-12x12-x0-y1-ports5-30s/summary.html`
- `heavy-xiranite-phase2-rotation-partition-12x12-x0-y1-ports5-30s/representative-layout.html`
