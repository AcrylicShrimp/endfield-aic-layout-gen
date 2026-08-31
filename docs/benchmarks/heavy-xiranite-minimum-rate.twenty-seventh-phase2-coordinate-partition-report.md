# Heavy Xiranite Minimum-Rate Phase 2 Coordinate Partition Report

## Question

Does the Phase 2 first-witness cliff disappear when the facility introduced by Phase 2 has a fixed coordinate, while rotation, every other placement, port selection, boundary terminals, routing, flow, topology, capacity, and collision remain exact solver decisions?

## Exact partition

The fixed 12 by 12 model was split into all 64 legal coordinates of the introduced 5 by 5 facility. Each case retained every legal rotation at that coordinate. The union of the cases is the original fixed-dimension feasible set, so the experiment does not exclude a legal solution.

The partitioned facility was:

`facility-instance:recipe-occurrence:/target/recipe:xiranite-oven-xiranite-enr-powder-1/input:item-xiranite-powder:0`

The preceding Phase 1 layout was solved first and used only as a non-binding placement hint. Its primary area optimum was proven and its selected bounds were 7 by 11.

## Result

| Metric | Result |
| --- | ---: |
| Fixed dimensions | 12 by 12 |
| Legal coordinates | 64 |
| Worker threads | 4 |
| Budget per coordinate | 1,000 ms |
| Outer wall time | 18,581 ms |
| Validated witnesses | 0 |
| Proven-infeasible coordinates | 16 |
| Unknown coordinates | 48 |
| Invalid witnesses | 0 |

The proven-infeasible cases cluster around the central overlap region and four corners, while most boundary and outer-ring coordinates remain unresolved. No coordinate produced a first incumbent.

Every executed case retained the Phase 2 model scale apart from one extra research equality:

| Model metric | Value |
| --- | ---: |
| Variables | 23,972 |
| Constraints | 82,626 |
| Factor-graph incidences | 285,109 |
| Placement-routing incidences | 83,454 |
| Facilities | 3 |
| Commodity networks | 8 |
| Route requirements | 10 |

## Conclusion

Fixing the introduced facility coordinate is not sufficient to break the first-witness cliff. This rejects the narrow hypothesis that the solver is spending nearly all of its time only choosing that facility's `(x, y)`.

The coordinate decision still sits above a large unresolved subproblem:

```text
fixed introduced-facility coordinate
  -> introduced-facility rotation and port choices
  -> endpoint geometry and boundary terminals
  -> shared routing, flow, topology, capacity, and collision
```

The 16 quick infeasibility proofs show that the coordinate fixation is propagating and is not being ignored. The remaining 48 one-second timeouts show that the dominant cliff lies below the coordinate decision.

## Next exact partition

Take one unresolved coordinate and partition the compatible port choice or choices of the introduced facility. Keep all other placements, boundary terminals, routing, flow, topology, capacity, and collision free. Enumerate every legal port alternative, so this remains semantics-preserving.

If this produces a fast witness, the next cliff is the introduced-facility coordinate/port to endpoint/routing channel. If it still times out, partition boundary-terminal choices or route endpoint attachment next.

## Artifacts

- `heavy-xiranite-phase2-coordinate-partition-12x12/summary.json`
- `heavy-xiranite-phase2-coordinate-partition-12x12/summary.html`
- `heavy-xiranite-phase2-coordinate-partition-12x12/representative-layout.html`
