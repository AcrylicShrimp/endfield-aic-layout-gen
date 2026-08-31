# Heavy Xiranite Minimum-Rate Phase 2 Port Partition Report

## Question

After fixing the Phase 2 introduced facility at one unresolved coordinate, does the first-witness cliff disappear when every compatible port assignment of that facility is partitioned exactly?

## Fixed context

- Exact used dimensions: 12 by 12.
- Introduced facility coordinate: `(0, 1)`.
- All preceding-facility placement, boundary-terminal, routing, flow, topology, capacity, and collision decisions remained free.
- The Phase 1 solution was reused only as a non-binding placement hint.

## Complete port domains

The introduced facility participates in four logical terminals:

| Terminal role | Compatible ports |
| --- | ---: |
| Pipe demand | 1 |
| Belt demand A | 5 |
| Belt demand B | 5 |
| Belt supply | 5 |

The complete Cartesian product contains `1 * 5 * 5 * 5 = 125` assignments. No compatible assignment was omitted.

## Result

| Metric | Result |
| --- | ---: |
| Port assignments | 125 |
| Worker threads | 4 |
| Budget per assignment | 1,000 ms |
| Outer wall time | 24,799 ms |
| Validated witnesses | 0 |
| Proven-infeasible assignments | 65 |
| Unknown assignments | 60 |
| Invalid witnesses | 0 |
| Fastest proof/search | 15 ms |
| Slowest search | 1,006 ms |

Each case added four research equalities to the same Phase 2 model:

| Model metric | Value |
| --- | ---: |
| Variables | 23,972 |
| Constraints | 82,630 |
| Factor-graph incidences | 285,113 |
| Placement-routing incidences | 83,454 |

## Conclusion

Fixing every port identity of the introduced facility is still insufficient to produce a first witness. The 65 fast infeasibility proofs show that port fixation propagates useful information, but the remaining 60 cases keep the dominant cliff.

One important geometry decision remains above routing: the facility rotation. Even at a fixed coordinate and with fixed port IDs, rotation changes the physical connection cells and outward directions of those ports. The next exact diagnostic should select one unresolved port assignment and partition every legal rotation at the same coordinate.

If all rotation cases remain unresolved, the introduced facility's physical geometry is no longer the primary unresolved decision and the next target becomes boundary-terminal placement or shared routing itself.

## Artifacts

- `heavy-xiranite-phase2-port-partition-12x12-x0-y1/summary.json`
- `heavy-xiranite-phase2-port-partition-12x12-x0-y1/summary.html`
- `heavy-xiranite-phase2-port-partition-12x12-x0-y1/representative-layout.html`
