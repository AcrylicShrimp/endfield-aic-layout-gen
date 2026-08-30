# Heavy Xiranite Minimum-Rate Bound-Sensitivity Report 3

## Question

How does the exact joint model for the same first cumulative SCC phase change when only the
caller-supplied square layout ceiling changes from 12 to 50 cells per side?

This is the third interactive research checkpoint. It measures the current formulation and stops
before selecting a reduction or changing solver semantics.

## Controlled Method

- Workload: `heavy-xiranite-minimum-rate`
- Target: `item-xiranite-enr-powder`, quantity 1 per 10,000 ms
- Cumulative phase: 0, always one facility, three commodity networks, eight terminals, and four
  route requirements
- Square request ceilings: 12, 16, 20, 24, 32, 40, and 50
- Formulation: `joint-lexicographic-layout-v4`
- Solver: Pumpkin 0.5, release build
- Search budget: 5,000 ms per case
- Repetitions: one per case

The research command selects the same first phase as the production SCC-growth planner but skips
the production-only final-graph facility-area preflight. The projected first phase still passes the
normal exact-model checks and retains every legal placement, port, component, and route choice
inside the requested bounds. The production solve path was not changed.

Each case has a static report, a recorded exact-model JSON report, a self-contained HTML failure or
solution view, and an external wall-time/memory record under
`heavy-xiranite-bound-sensitivity/`. The normalized cross-case data is in
`heavy-xiranite-bound-sensitivity.summary.json`.

The exact case for side `N` can be reproduced from a release build with:

```bash
target/release/aic-cli research solve-first-phase \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.NxN.request.json \
  --time-limit-ms 5000 \
  --output docs/benchmarks/heavy-xiranite-bound-sensitivity/phase0.NxN.json \
  --visualization-output docs/benchmarks/heavy-xiranite-bound-sensitivity/phase0.NxN.html
```

Replace both occurrences of `N` with one measured side. The command intentionally returns a
non-zero process status for `unknown` after writing both artifacts.

## Results

All seven cases reached the five-second limit without a complete incumbent. Their status is
`unknown`, not infeasible, and no heuristic fallback ran.

| Ceiling | Variables | Constraints | Incidences | Build | Search | Wall | Peak RSS | First incumbent |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 12 by 12 | 26,913 | 96,652 | 328,135 | 128 ms | 5,001 ms | 5.15 s | 101.7 MiB | none |
| 16 by 16 | 50,401 | 189,148 | 664,927 | 245 ms | 5,001 ms | 5.27 s | 156.3 MiB | none |
| 20 by 20 | 81,377 | 313,388 | 1,123,095 | 418 ms | 5,001 ms | 5.45 s | 252.4 MiB | none |
| 24 by 24 | 119,841 | 469,372 | 1,702,639 | 650 ms | 5,000 ms | 5.70 s | 392.1 MiB | none |
| 32 by 32 | 219,233 | 876,572 | 3,225,855 | 1,285 ms | 5,001 ms | 6.38 s | 650.6 MiB | none |
| 40 by 40 | 348,577 | 1,410,748 | 5,234,575 | 2,220 ms | 5,066 ms | 7.42 s | 1,036.9 MiB | none |
| 50 by 50 | 552,377 | 2,257,028 | 8,428,215 | 3,950 ms | 5,099 ms | 9.27 s | 1,705.2 MiB | none |

The model is already hard enough to prevent a first complete assignment at 12 by 12. Increasing
the ceiling then adds construction and memory cost before search receives the same five-second
budget. One repetition is sufficient to establish structural counts, but timing and memory values
are observations rather than stable performance estimates.

## Exact Growth Decomposition

Let `N` be the side length of a measured square ceiling. Across all seven cases, the recorded total
matches these polynomials exactly:

```text
variables   = 234 N^2 -   680 N +  1,377
constraints = 992 N^2 - 4,652 N +  9,628
terms       = 3,921 N^2 - 23,158 N + 48,575
incidences  = 3,793 N^2 - 22,006 N + 46,015
```

These identities describe this phase, its current data, and the measured square bounds. They are
not game-wide complexity laws.

The variable formula is directly attributable to recorded families:

| Variable family | Count for side `N` | 12 by 12 | 50 by 50 |
| --- | ---: | ---: | ---: |
| Placement origins and rotations | `4(N - 4)^2` | 256 | 8,464 |
| Directional facility endpoints | `64(N - 4)(N - 5)` | 3,584 | 132,480 |
| Route cells | `3N^2` | 432 | 7,500 |
| Directed route arcs | `12N(N - 1)` | 1,584 | 29,400 |
| Flow on directed arcs | `12N(N - 1)` | 1,584 | 29,400 |
| Route order | `3N^2` | 432 | 7,500 |
| Terminal presence, arms, and branch components | `72N^2` | 10,368 | 180,000 |
| Bridges, rotations, and crossing owners | `16N^2` | 2,304 | 40,000 |
| Objective auxiliaries | `48N(N - 1) + 33` | 6,369 | 117,633 |

The asymptotic `234N^2` coefficient is therefore not one undifferentiated routing grid. Its largest
pieces are directional endpoint choices (`64N^2`), terminal/arm/component topology (`72N^2`),
objective auxiliaries (`48N^2`), and route-arc plus flow variables (`24N^2`). Facility placement is
only `4N^2` in this one-facility phase.

The original static phase-zero counter excluded objective auxiliaries. Its lower bound rose from
20,544 variables at 12 by 12 to 434,744 at 50 by 50. The recorder added exactly the missing 6,369
to 117,633 objective variables at the corresponding bounds; every previously covered family still
matched.

## Coupling Growth

The factor graph remains one connected component at every bound. The maximum variable degree stays
141, while maximum constraint arity grows from 3,745 to 65,001. Density decreases because the graph
gets larger, but that does not make it separable: direct placement-routing constraints grow from
29,444 to 981,040.

For the measured series, direct placement-routing constraints match:

```text
465 N^2 - 3,788 N + 7,940
```

At 50 by 50 they represent 43.47% of all posted constraints and contain 4,622,236 incidences. The
growth is caused primarily by endpoint-to-grid compatibility, route occupancy, collision, and
component-topology links. The solver is still choosing placement and routing together; changing the
ceiling expands their joint coupling rather than merely adding passive empty canvas.

Constraint-family growth is also distributed. Between 12 and 50, the largest 50 by 50 families
are bridge crossing (394,960), branch topology (352,500), route-cell activation (331,260), terminal
presence (324,960), turn definition (312,996), and used geometry (234,202). No single family alone
accounts for the 2.26 million constraints.

## Resource Scaling

From 12 to 50, grid area grows 17.36 times, variables grow 20.53 times, constraints grow 23.35
times, and factor-graph incidences grow 25.69 times. Boundary effects explain why the per-cell
counts rise toward their quadratic leading coefficients:

| Ceiling | Variables per cell | Constraints per cell | Incidences per cell |
| --- | ---: | ---: | ---: |
| 12 by 12 | 186.90 | 671.19 | 2,278.72 |
| 24 by 24 | 208.06 | 814.88 | 2,955.97 |
| 50 by 50 | 220.95 | 902.81 | 3,371.29 |

Peak RSS rises from 106.6 MB to 1.79 GB, approximately in proportion to grid area over this range.
Recorded model construction grows from 128 ms to 3,950 ms, a 30.9-times increase. The recorder is
part of these costs, so this experiment does not isolate Pumpkin model allocation from measurement
overhead.

## Supported Conclusions

1. The production preflight leakage was real: it had rejected small research ceilings based on the
   final 59-facility graph. The research-only path now confirms that every measured model contains
   exactly one phase-zero facility while production behavior remains unchanged.
2. The ceiling controls much more than possible facility origins. It quadratically expands route,
   endpoint, component, collision, and objective structures even though the logical production
   subgraph is unchanged.
3. The largest variable coefficients come from endpoint and network-topology encodings, not the
   placement variables themselves.
4. The one-component factor graph and rapidly growing direct coupling show that a plain connected-
   component split cannot separate placement from routing in this formulation.
5. A smaller bound reduces model construction and memory substantially, but 12 by 12 still gives
   no incumbent in five seconds. Bound size is therefore a major cost multiplier, but it is not a
   sufficient explanation for the failure to find a first solution.

The experiment does **not** show whether 12 by 12 is feasible, which family dominates Pumpkin's
search, or whether any particular reformulation will improve search. It also does not justify an
unproved crop, candidate restriction, port preselection, or routing heuristic.

## Next Decision Gate

The next research step should be chosen interactively. This checkpoint exposes three distinct
questions:

- **First-incumbent diagnosis:** instrument or isolate the first objective stage on the 12 by 12
  case to learn why even the smallest measured model cannot produce a complete assignment.
- **Exact endpoint reformulation:** compare semantics-preserving encodings for the `64(N-4)(N-5)`
  endpoint family and its high-arity choices.
- **Exact routing-local reformulation:** compare smaller or stronger equivalent encodings for the
  route-cell, topology, collision, and objective families that dominate constraints.

No option has been selected or implemented by this report.
