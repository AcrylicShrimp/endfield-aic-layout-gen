# Heavy Xiranite V2 Cumulative Growth Report

## Outcome

The exact v2 formulation reaches its first cumulative SCC incumbent cliff at phase 3 under the
controlled 12 by 12 request ceiling and a fresh 5,000 ms search budget per phase.

- Phase 0 proves the optimum in 1,882 ms.
- Phase 1 finds its first incumbent in 1,179 ms but does not prove the optimum.
- Phase 2 finds one validated incumbent in 4,174 ms.
- Phase 3 constructs successfully but finds no incumbent in 5,003 ms. The result is `unknown`, not
  infeasible.

The transition is from three to four facilities and from ten to thirteen logical transport
requirements. The number of shared physical routing layers remains two and the number of commodity
networks remains two. This is the first measured enlarged-graph cliff for the v2 exact architecture.

The phase-2 result initially exposed two witness-validator contract defects rather than search
failures. The accepted circulation contract still had an obsolete terminal-path reachability check,
and the crossing validator confused four-arm splitter/converger cells with perpendicular bridge
crossings. Both were repaired without changing solver feasibility or the objective, after which the
same phase-2 incumbent passed independent validation.

## Controlled Experiment

- Workload: Heavy Xiranite minimum rate.
- Request ceiling: 12 by 12; this is an experiment input, not a project limit.
- Formulation: `joint-shared-transport-layer-external-connectors-v2`.
- Growth order: cumulative output-first SCC phases.
- Search budget: 5,000 ms independently for each phase.
- Build: optimized release binary.
- Process isolation: one fresh process per target phase.
- Hint: common prior facility placements only, non-binding.
- Stop rule: first phase without a complete validated incumbent.

Every phase retains the full legal placement, rotation, port, connector, component, and routing
domains. No corridor, candidate crop, fixed prior coordinate, deterministic placement, or heuristic
fallback was used.

## Cumulative Result

Objective vectors are `(used area, physical transport tiles, route turns, maximum side, logistics
components)`.

| Phase | Facilities | Requirements | Networks | External connectors | First incumbent | Search | Result | Objective | Validation | Peak RSS |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | ---: |
| 0 | 1 | 4 | 0 | 4 | 78 ms | 1,882 ms | optimal | (42, 4, 0, 7, 0) | passed | 77.27 MiB |
| 1 | 2 | 7 | 1 | 6 | 1,179 ms | 5,005 ms | feasible | (84, 39, 10, 12, 6) | passed | 192.30 MiB |
| 2 | 3 | 10 | 2 | 8 | 4,174 ms | 5,010 ms | feasible | (144, 112, 47, 12, 14) | passed | 263.75 MiB |
| 3 | 4 | 13 | 2 | 10 | none | 5,003 ms | unknown | none | not attempted | 274.70 MiB |

Phase 2 already uses the complete 12 by 12 occupied bounding box and has only one incumbent. That
does not prove phase 3 infeasible within this ceiling. Pumpkin produced neither an incumbent nor an
infeasibility proof, so the phase-3 outcome remains a pure search-budget result.

## Search-Space Growth

| Phase | Variables | Boolean | Integer | Log2 domain volume | Constraints | Terms | Build |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 15,770 | 15,734 | 36 | 15,968.16 | 70,522 | 201,137 | 88 ms |
| 1 | 32,622 | 31,755 | 867 | 33,989.50 | 134,604 | 414,263 | 159 ms |
| 2 | 48,573 | 47,391 | 1,182 | 50,466.63 | 194,660 | 630,619 | 258 ms |
| 3 | 56,452 | 55,243 | 1,209 | 58,520.48 | 231,793 | 764,169 | 311 ms |

The phase-2 to phase-3 delta is:

| Metric | Delta | Relative growth |
| --- | ---: | ---: |
| Variables | +7,879 | +16.2% |
| Boolean variables | +7,852 | +16.6% |
| Integer variables | +27 | +2.3% |
| Log2 domain volume | +8,053.85 | +16.0% |
| Constraints | +37,133 | +19.1% |
| Terms | +133,550 | +21.2% |

External-connector state accounts for 6,954 of the 7,879 added variables, or 88.3%. Its three
recorded constraint relations add 28,491 constraints, or 76.7% of the total constraint delta, and
71,856 terms, or 53.8% of the term delta. Used-geometry constraints add another 6,688 constraints
and 20,064 terms. Transport-collision constraints keep the same count but gain 25,888 terms because
the new facility and connector candidates participate in the same occupied-cell sums.

The model still contains only 288 shared route-cell variables and 1,056 shared route-arc variables
at both phases 2 and 3. The cliff is therefore not caused by adding another physical routing grid.
It occurs while adding one facility, two external connectors, two more internal terminals, and the
new placement-to-connector/collision coupling around the same two shared layers.

## Current Placement Representation

The current code does not have independent solver variables for `x`, `y`, and `rotation`.

`generate_candidates` enumerates every legal `(rotation, x, y)` tuple and creates one Boolean
`candidate.selected` variable for each tuple. For the 5 by 5 facilities in this 12 by 12 experiment,
four rotations times 8 by 8 origins produce 256 Boolean candidates per facility.

The v2 endpoint encoding adds one integer `placement_choice` with domain `0..255`, but this value is
only the row index of the selected tuple. The channel is:

```text
sum(candidate[i]) = 1
placement_choice = sum(i * candidate[i])
```

Both equations are posted through `pumpkin_solver::equals`. Pumpkin 0.5 implements an arithmetic
equality as two linear inequalities. There is no dedicated tuple-table, element inverse, or
independent coordinate channel.

This answers the placement-propagation questions directly:

1. `x`, `y`, and `rotation` are not independent domains.
2. Their values exist primarily inside placement candidate records; the integer choice stores only
   the combined tuple index.
3. There is no `x = N` domain value whose removal immediately removes every tuple at that x. Those
   candidate Booleans can only be eliminated through the individual constraints in which they
   participate.
4. Removing a candidate does not shrink independent x/y/rotation domains because those domains do
   not exist. It may tighten the lower or upper bound of the combined candidate index when an edge
   of that index range disappears, but an internal removed tuple does not create an equivalent
   coordinate-domain propagation event.
5. Candidate selection uses one linear exactly-one equation. Candidate-to-index channeling uses one
   wide weighted linear equality per facility. Downstream v2 port geometry is selected from that
   index through variable-element constraints.

This structure is a credible propagation target, but the current matrix does not prove that it is
the phase-3 cause. Placement itself adds only 257 variables in the transition, while connector
state dominates the raw delta. The earlier port-selected-element result also demonstrated that raw
counts can be misleading: a slightly larger but better factored exact model removed a five-second
cliff. The placement hypothesis therefore requires a controlled exact reformulation, not a size-only
conclusion.

## Improvements Retained In This Baseline

The current v2 matrix includes all accepted exact improvements from the preceding research:

1. Structured model/search diagnostics, release-mode timing, isolated RSS, JSON artifacts, and
   self-contained HTML for success and failure.
2. Removal of the expensive cycle-free proof while retaining positive flow, terminal balance,
   conservation, capacity, topology, and collision constraints.
3. Cancellation of exactly co-located equal-flow source/sink terminals as a proven-safe dominance
   rule.
4. One exact shared belt layer and one exact shared pipe layer instead of a dense routing grid per
   logical network.
5. Independent placement and port selection instead of a flattened placement-times-port endpoint
   Boolean domain.
6. Straight external boundary connectors with three solver-selected legal side templates instead
   of routed external pseudo-terminals.
7. Port-selected variable-element geometry instead of a scalar placement-times-port Cartesian
   lookup index.
8. Cumulative SCC growth with a complete enlarged exact model and placement-only, non-binding prior
   solution hints.
9. Independent witness validation aligned with the accepted circulation and four-arm branch
   topology contracts.

No item in this list fixes placement, port, or route decisions outside the solver.

## What The Evidence Rules Out

- Model construction is not the immediate cliff: phase 3 builds in 311 ms.
- Another per-network dense grid was not introduced: route-cell and route-arc counts are unchanged
  from phase 2.
- The result is not a validator rejection: phase 3 has no incumbent to validate.
- The result is not proven infeasibility under 12 by 12.
- Placement hints do not restrict the enlarged model and therefore do not explain missing legal
  solutions.
- Raw placement-variable growth alone does not explain the delta; most added state belongs to
  external connectors and their coupling. Propagation quality remains an open explanation.

## Recommended Next Experiment

The next checkpoint should compare the current candidate-index placement channel with one exact
coordinate-factored placement channel on phases 2 and 3 under the same 5,000 ms budget.

The alternative must introduce explicit `x`, `y`, and `rotation` domains and channel them
bidirectionally to the complete legal tuple set. Connector and port geometry should consume those
factored decisions without recreating a flattened `(rotation, x, y, port)` scalar. Candidate
Booleans may remain where exact footprint occupancy needs them, but removing an x/y/rotation value
must remove every incompatible candidate, and removing candidates must prune every unsupported
coordinate value. The comparison must verify identical legal tuples and objective semantics before
measuring first-incumbent time.

This is a semantics-preserving reformulation candidate, not an approved implementation yet. The
experiment should report root propagation behavior, model-family deltas, phase-2 first-incumbent
time and objective, and whether phase 3 gains a validated incumbent. If it does not, the next split
should isolate the external-connector-to-placement and collision coupling, which accounts for most
of the measured phase-3 growth.

## Validator Corrections During The Run

- `a5374c9` removes the obsolete directed terminal-path reachability rejection while retaining all
  accepted flow constraints.
- `139a3fb` distinguishes valid four-arm splitter/converger cells from two-channel bridge crossings.

Pre-correction phase-2 JSON, HTML, and time records are preserved with `validator-rejected` and
`branch-rejected` suffixes. They are diagnostic evidence, not solver failures.

## Verification

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`: 175 tests passed
- optimized release-mode cumulative phases 0 through 3
- independent witness validation for every successful phase
- self-contained HTML for every target, including phase-3 failure evidence
- `/usr/bin/time -l` process elapsed time and peak RSS for every target

## Artifacts

- Experiment contract:
  `docs/designs/v2-cumulative-scc-growth-experiment.md`
- JSON, HTML, and process-time records:
  `docs/benchmarks/heavy-xiranite-v2-cumulative-scc-growth/`
- Previous accepted endpoint reformulation:
  `docs/benchmarks/heavy-xiranite-minimum-rate.fifteenth-port-selected-element-report.md`

## Decision Boundary

The faithful v2 cumulative-growth cliff is established at phase 3. This checkpoint does not apply
the coordinate-factored placement reformulation or any other reduction. Review the evidence before
choosing the next controlled experiment.
