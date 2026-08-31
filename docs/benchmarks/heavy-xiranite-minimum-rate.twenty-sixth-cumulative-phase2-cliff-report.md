# Heavy Xiranite Cumulative Phase-Two Cliff Report

## Outcome

The first cumulative exact dimension growth cliff occurs at SCC phase 2.

Phase 0 and Phase 1 still complete with validated incumbents and proven minimum areas. Phase 2
contains three facilities, eight commodity networks, and ten route requirements. It produces no
validated witness under either a 5-second or a 30-second per-dimension budget.

This is not an infeasibility result. Under the 30-second budget, Pumpkin proves every exact
dimension through area 121 infeasible but leaves `11x12`, `12x11`, and `12x12` unknown.

## Baby-Mode Summary

The solver now knows this much:

```text
Everything up to 11x11 is too small.
```

Only three factory shapes remain possible inside the caller's `12x12` ceiling:

```text
11x12
12x11
12x12
```

For each of those shapes, the solver searched for 30 seconds. It did not find a layout, but it also
could not prove that no layout exists. Therefore the correct answer is `unknown`.

## Five-Second Result

The four-worker run completed the previous phases and attempted all 26 Phase 2 dimension cases.

| Result | Cases |
| --- | ---: |
| Validated feasible | 0 |
| Proven infeasible | 20 |
| Unknown | 6 |
| Invalid witness | 0 |

The unknown cases were `10x12`, `12x10`, `11x11`, `11x12`, `12x11`, and `12x12`. Phase 2 sweep
wall time was 14,134 ms. The complete process took 18.41 seconds and 921.70 MiB peak RSS.

## Thirty-Second Result

The longer run moved the proof boundary but did not find a first witness.

| Dimensions | Area | Result | Search time |
| --- | ---: | --- | ---: |
| `10x12` | 120 | proven infeasible | 7,136 ms |
| `12x10` | 120 | proven infeasible | 7,430 ms |
| `11x11` | 121 | proven infeasible | 14,353 ms |
| `11x12` | 132 | unknown | 30,005 ms |
| `12x11` | 132 | unknown | 30,009 ms |
| `12x12` | 144 | unknown | 30,005 ms |

Phase 2 sweep wall time was 41,451 ms. The complete process took 45.85 seconds and 964.44 MiB
peak RSS. There is still no feasible area upper bound.

## What Grew

| Metric | Phase 1 | Phase 2 | Change |
| --- | ---: | ---: | ---: |
| Facilities | 2 | 3 | +1 |
| Commodity networks | 6 | 8 | +2 |
| Route requirements | 7 | 10 | +3 |
| Variables | 21,745 | 23,972 | +2,227 |
| Constraints | 71,395 | 82,625 | +11,230 |
| Constraint incidences | 239,999 | 285,105 | +45,106 |
| Log2 domain volume | 25,717.03 | 28,621.98 | +2,904.96 |
| Placement-routing constraints | 14,188 | 19,736 | +5,548 |
| Placement-routing incidences | 58,334 | 83,454 | +25,120 |

The raw model grows by only about ten percent in variables, but the newly added state is highly
coupled to routing.

## Variable-Family Breakdown

All 2,227 additional variables are explained by five families:

| Variable family | Added variables | Added log2 volume | Meaning |
| --- | ---: | ---: | --- |
| Endpoint geometry | 1,196 | 1,359.44 | Selected facility ports mapped onto candidate cells |
| Boundary terminal | 626 | 642.34 | External terminal positions on the chosen used boundary |
| Placement | 257 | 264.00 | Third facility placement candidates and choice |
| Physical occupancy | 144 | 144.00 | Third facility footprint occupancy channels |
| Endpoint choice | 4 | 6.97 | Additional directional port selections |

The existing shared-layer routing variable counts do not grow:

- route cells: no change;
- route arcs: no change;
- flow variables: no change;
- route arms: no change;
- objective variables: no change.

The arm-item variable count also remains constant, although its item domain gains about 488.21 bits
of log2 volume because two more commodities enter the shared layer.

## Constraint-Family Breakdown

The largest added constraint groups are:

| Constraint family | Added constraints |
| --- | ---: |
| Bridge crossing | 2,376 |
| Branch topology | 2,304 |
| Line capacity | 2,208 |
| Item assignment | 1,800 |
| Terminal presence | 1,800 |
| Boundary terminal | 578 |
| Occupancy channel | 144 |
| Endpoint link | 20 |

Almost the entire increase is cross-family coupling. Grid shape and route-cell mechanics are not
being replicated per network again.

## Root-Cause Boundary

The measured cliff is not caused by a regression to one dense grid per network. The shared belt
and pipe layers remain fixed at the `12x12` request ceiling.

The next suspect is the combined choice of:

```text
third facility placement
    -> directional facility port
    -> endpoint cell geometry
    -> external boundary terminal
    -> shared route item, flow, topology, and capacity state
```

The third facility adds relatively few independent variables but connects them to many existing
shared-layer constraints. The 43 percent rise in placement-routing incidences is stronger evidence
than total variable count alone.

This identifies the target family, not one proven defective constraint. A controlled exact
partition is still required to identify which link creates the first-witness cliff.

## Recommended Next Experiment

Keep only the remaining exact dimensions `11x12`, `12x11`, and `12x12`. Inside each case, partition
one high-level decision family at a time while enumerating every legal value:

1. partition the newly introduced facility's placement candidate;
2. if the cliff remains, partition its directional port choice;
3. then partition boundary-terminal side before exact terminal cell;
4. keep all routing, flow, topology, and collision decisions free inside every partition.

This remains semantics-preserving only if every legal value is included and no partition is
silently skipped. The experiment should measure first witness, infeasibility proof, model
propagation, and aggregate proof completeness.

Do not add a placement heuristic, preferred port, coordinate window, route corridor, or fixed
terminal rule. Stop here for user review before implementing this next decomposition.

## Verification And Artifacts

- implementation commit: `c1d39c3`;
- Phase 1 result commit: `1205e00`;
- optimized release binary;
- `12x12` caller ceiling;
- four independent workers;
- complete 5-second and 30-second per-case runs;
- automatic JSON and HTML for every executed case, including unknown results;
- no invalid witnesses;
- machine-readable comparison:
  `heavy-xiranite-cumulative-dimension-sweep-phase2/comparison.json`;
- 5-second history: `heavy-xiranite-cumulative-dimension-sweep-phase2/summary.json` and
  `summary.html`;
- 30-second history: `heavy-xiranite-cumulative-dimension-sweep-phase2-30s/summary.json` and
  `summary.html`.
