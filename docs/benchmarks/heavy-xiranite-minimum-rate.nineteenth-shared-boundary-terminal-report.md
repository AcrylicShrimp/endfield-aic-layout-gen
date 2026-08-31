# Heavy Xiranite Shared Boundary Terminal Cutover Report

## Outcome

External inputs and outputs now use the same exact belt and pipe commodity networks as internal
facility connections. The previous dedicated straight-ray connector subsystem and its imposed
shape are deleted. A boundary terminal, facility placement, rotation, facility port, shared route,
branch, and convergence remain solver decisions. No replacement layout or routing heuristic was
introduced.

The architecture is simpler and its solution set is less restricted, but the faithful search is
harder. The Heavy Xiranite phase-0 model found no first complete incumbent in either 5 seconds or
30 seconds. This is the new measured cliff. It is not infeasibility and no fallback was attempted.

## Contract Change

The old model treated every external requirement as a private physical object:

```text
facility port -> one solver-selected straight ray -> fixed blueprint boundary
```

It excluded external edges from the shared commodity networks. The new model treats an external
endpoint as an ordinary source or sink terminal:

```text
solver-selected boundary terminal
  -> ordinary shared belt or pipe commodity network
  -> solver-selected facility port
```

The terminal cell must lie on the actual used bounding box and select the corresponding outward
side. The route between it and the facility is unrestricted ordinary network geometry. Same-item
external and internal flow may share trunks, split, and converge.

The output schema is a hard cutover. External terminals now appear only inside
`transport_networks`; the obsolete top-level `external_connectors` collection and connector-owned
cells are removed.

## Why The Shape Heuristic Is No Longer Needed

The accepted objective already defines the desired preference after a complete solution exists:

1. minimum used bounding-box area;
2. minimum physical belt and pipe tile count;
3. minimum total route turns;
4. later exact tie-breakers.

A dedicated straight connector was therefore not needed to express layout quality. It also removed
legal shared-network shapes and duplicated physical state per requirement. The cutover restores the
solver's authority over the external route while retaining the same compactness objective.

The objective does not, however, guarantee a fast first incumbent. Before the solver has a complete
feasible placement and flow network, it cannot compare candidate objective vectors. It must first
resolve enough placement, port, boundary-terminal, item, direction, capacity, topology, and flow
state to construct one valid witness. The new phase-0 timeout occurs before objective-driven
incumbent improvement begins.

## Release-Mode Phase-0 Comparison

Both rows use the same Heavy Xiranite minimum-rate graph and the same caller-supplied 12 by 12 hard
ceiling. The v3 baseline imposed private straight external connectors. The v4 model routes all four
external requirements through three shared commodity networks: two belt items and one pipe item.

| Metric | Straight connector v3 | Shared boundary terminal v4 |
| --- | ---: | ---: |
| Facilities | 1 | 1 |
| Route requirements | 4 | 4 |
| Routed commodity networks | 0 | 3 |
| Dedicated external/boundary variables | 15,336 | 1,252 |
| Total variables | 16,346 | 19,582 |
| Boolean / integer variables | 16,310 / 36 | 17,194 / 2,388 |
| Log2 domain volume | 16,543.17 | 22,480.84 |
| Constraints | 64,408 | 57,843 |
| Terms | 162,945 | 186,379 |
| Construction | 66 ms | 138 ms |
| First incumbent | 16 ms | none in 5,000 ms |
| Final result | optimal `(42, 4, 0)` | unknown |

The dedicated external decision family fell by 14,084 variables, or 91.84%. Total constraints also
fell by 10.19%. Total variables nevertheless rose by 19.80% and terms by 14.38% because phase 0 now
contains the real shared route, flow, item, topology, bridge, and turn state for three commodity
networks. The old phase-0 model did not build those networks at all; every requirement was external
and was diverted into the special connector subsystem.

## Extended Search

The same v4 model was given 30,000 ms in an isolated optimized release process.

| Budget | Build | Search | First incumbent | Incumbents | Termination | Peak RSS |
| ---: | ---: | ---: | ---: | ---: | --- | ---: |
| 5 s | 138 ms | 5,000 ms | none | 0 | unknown | 127.48 MiB |
| 30 s | 121 ms | 30,001 ms | none | 0 | unknown | 150.56 MiB |

The model is unchanged between the two runs: 19,582 variables, 57,843 constraints, and 186,379
terms. The lack of an incumbent after 30 seconds makes this a pre-incumbent feasibility-search
cliff, not an objective tie-breaking problem. It does not prove the request infeasible.

## What Became Simpler

- One routing meaning now covers internal and external material flow.
- External requirements no longer own private grid masks or route templates.
- The 868-line dedicated connector model was deleted.
- Same-item external and internal flow can share physical trunks.
- Witness validation and HTML consume the same network-terminal representation.
- The solver output no longer needs a second top-level physical connector collection.

The implementation cutover changed 398 lines and deleted 1,247 lines in the main architecture
commit.

## Current Exact Baseline

The current v4 baseline retains the previously accepted exact improvements:

1. Structured build/search diagnostics and self-contained HTML for both success and failure.
2. Circulation-permitted flow semantics without a separate cycle-free proof.
3. Exact cancellation of co-located equal-flow source and sink terminals.
4. One shared belt layer and one shared pipe layer.
5. Independent placement and port decisions.
6. Port-selected variable-element geometry.
7. Cumulative SCC growth with non-binding hints only.
8. Canonical physical occupancy with bidirectional placement/transport channeling.
9. Shared commodity-network boundary terminals with no imposed route shape.

No placement, rotation, port, boundary point, or route is preselected by the harness.

## Next Exact Diagnosis Target

The next narrow target is the interaction that prevents the first incumbent before optimization:

```text
facility placement and port
  <-> boundary terminal and used bounding box
  <-> shared item/direction/topology state
  <-> integer flow conservation and capacity
```

The next experiment should decompose this coupling on the same phase-0 graph. It should identify
which exact family or combination causes the jump from the previous 16 ms incumbent to no incumbent
in 30 seconds. Candidate exact reformulations may strengthen propagation or replace repeated
relations, but must preserve every legal placement, boundary terminal, and network shape. The
result does not justify restoring straight rays, fixing a side, restricting a corridor, or adding
another heuristic fallback.

## Verification

- `cargo fmt --all`
- `cargo test --workspace`: 177 tests passed
- `cargo build --release --workspace`
- optimized release-mode phase-0 solve with 5,000 ms search
- optimized release-mode phase-0 solve with 30,000 ms search
- structured `unknown` diagnostics and automatically generated HTML for both timeouts
- isolated `/usr/bin/time -l` peak RSS measurement
- no heuristic fallback

## Artifacts

- Contract: `docs/designs/shared-boundary-terminal-cutover.md`
- Machine-readable comparison:
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminals/comparison.json`
- Five-second JSON and HTML:
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminals/12x12-phase0.json` and
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminals/12x12-phase0.html`
- Thirty-second JSON and HTML:
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminals/12x12-phase0.30s.json` and
  `docs/benchmarks/heavy-xiranite-shared-boundary-terminals/12x12-phase0.30s.html`

## Decision Boundary

The shared-boundary-terminal architecture is implemented and verified as the new faithful exact
baseline. Its first Heavy Xiranite result is a structured phase-0 timeout, which is an expected and
useful research result. Review this checkpoint before changing the formulation again. The next
work should diagnose the new pre-incumbent cliff, not weaken the accepted network semantics.
