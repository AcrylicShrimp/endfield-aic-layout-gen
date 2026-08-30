# Heavy Xiranite Requirement Decomposition

## Scope

This checkpoint rebuilds the hard phase-zero Xiranite Powder network from each logical route
requirement and from the combined pair. All cases use the exact factored shared-layer formulation.
No decision is fixed and no constraint family is removed.

## Five-Second Matrix

| Case | Requirements | Terminals | Endpoint variables | Total variables | Constraints | Terms | Factor incidences | Placement-routing incidences | First incumbent ms | Result |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `requirement-1` | 1 | 2 | 311 | 8,793 | 31,239 | 110,344 | 110,036 | 29,704 | 2,968 | optimal, validated |
| `requirement-2` | 1 | 2 | 311 | 8,793 | 31,239 | 110,344 | 110,036 | 29,704 | 3,118 | optimal, validated |
| `combined` | 2 | 4 | 622 | 9,104 | 32,781 | 117,741 | 117,125 | 36,788 | 4,304 | unknown, invalid witness |

The three-case batch peak was 123,240,448 bytes according to macOS `/usr/bin/time -l`. It is a
batch peak, not an isolated per-case RSS measurement.

Both individual requirements are structurally identical at the recorded model level and solve to
proven optimality. The cliff therefore requires their simultaneous presence in the same commodity
network; neither route requirement is intrinsically hard by itself.

## Exact Growth at the Boundary

Adding the second requirement does not add any route-cell, route-arc, flow, arm, branch-component,
bridge, or objective variable. It adds only terminal selection state:

- 2 endpoint selector variables;
- 309 endpoint-geometry variables;
- 1,542 constraints;
- 7,397 constraint terms;
- 7,089 factor incidences; and
- 7,084 placement-routing incidences.

The added constraints and terms are distributed as follows:

| Constraint family | Added constraints | Added terms |
| --- | ---: | ---: |
| Terminal presence | 616 | 1,848 |
| Item assignment | 616 | 1,232 |
| Bridge crossing | 308 | 616 |
| Endpoint link | 2 | 5 |
| Flow conservation | 0 | 616 |
| Branch topology | 0 | 2,464 |
| Line capacity | 0 | 616 |

The dominant expansion is not another routing grid. It is one additional facility-port/external
terminal pair being connected to every reachable cell-direction candidate of the existing shared
grid.

## Longer-Budget Control

The same matrix was repeated with an independent 15-second budget. The combined case became
validated and proven optimal after 8,076 ms:

| Objective stage | Incumbent | Search ms | Proof |
| --- | ---: | ---: | --- |
| Used bounding-box area | 30 | 5,582 | proven optimal |
| Physical transport tiles | 2 | 2,218 | proven optimal |
| Total route turns | 0 | 14 | proven optimal |
| Maximum used side | 6 | 244 | proven optimal |
| Logistics components | 0 | 9 | proven optimal |

At five seconds, the solver had already found 13 incumbents, but it had not completed the primary
area stage. Because circulation is solver-legal and transport-tile minimization had not started,
the retained incumbent contained active geometry outside a directed supply-to-demand path. The
post-solve validator rejected that intermediate witness. This is a model/validator contract gap,
not evidence of infeasibility.

## Current Conclusion and Next Iteration

The first local cliff is now narrowed to the second terminal pair's placement-port-to-grid coupling.
Routing state itself is constant across the boundary. The next diagnostic must determine whether
the search cost comes primarily from free endpoint/port geometry or from propagating those choices
through flow, capacity, item, and topology constraints on the shared grid. A fixed-endpoint versus
free-routing diagnostic can answer that question; any fixed result is diagnostic-only and must not
be used as a production layout.

## Artifacts

- Five-second cases: `docs/benchmarks/heavy-xiranite-factored-requirement-decomposition/`
- Fifteen-second control: `docs/benchmarks/heavy-xiranite-factored-requirement-decomposition-15s/`
