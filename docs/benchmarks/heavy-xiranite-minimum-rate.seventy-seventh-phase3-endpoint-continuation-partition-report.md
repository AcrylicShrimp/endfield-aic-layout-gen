# Heavy Xiranite Phase 3 Endpoint-Continuation Partition

## Result

Fixing the canonical first positive source arc and last positive demand arc of the selected
final-product belt network is not sufficient to cross the five-second Phase 3 first-feasible cliff
in this concurrent run. The exact
partition has two source cases and two demand cases. All four Cartesian-product cases remain
`Unknown`:

| Case | Source arc | Earlier source arcs fixed to zero | Demand arc | Earlier demand arcs fixed to zero | Authoritative | Observation | Search ms | Decisions | Backtracks | Conflicts | Learned | Propagations |
| ---: | --- | ---: | --- | ---: | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | `70 -> 54` | 0 | `7 -> 6` | 0 | Unknown | Unknown | 5,006 | 40,683 | 2,845 | 2,844 | 2,844 | 3,995,664 |
| 1 | `70 -> 54` | 0 | `22 -> 6` | 1 | Unknown | Unknown | 5,007 | 42,230 | 3,061 | 3,060 | 3,060 | 4,023,855 |
| 2 | `70 -> 71` | 1 | `7 -> 6` | 0 | Unknown | Unknown | 5,006 | 43,884 | 2,999 | 2,998 | 2,998 | 3,994,699 |
| 3 | `70 -> 71` | 1 | `22 -> 6` | 1 | Unknown | Unknown | 5,006 | 43,897 | 3,303 | 3,302 | 3,302 | 3,826,445 |

No case produced an incumbent, an infeasibility proof, an invalid witness, or conflicting
evidence. Every interpretation gate passed.

The cases ran concurrently, so their search counters describe the work completed under the same
wall-clock cutoff but do not support absolute per-case speed rankings.

This result rejects only the narrow hypothesis that the selected network's immediate endpoint
continuation choice is sufficient to explain the controlled cliff. It does not prove that endpoint
continuation is irrelevant. The cases remain cutoff-censored, and the route interior, seven other
material networks, optional logistics components, topology, and flow coupling remain free.

## Exact Experiment Contract

The controlled parent is the accepted `16 x 16` cumulative Phase 3 leaf with:

- four facility placements and rotations fixed;
- fifteen facility ports fixed;
- the final-product external demand fixed to north boundary key 24;
- the selected network fixed to `network:belt:item-xiranite-enr-powder`;
- every route-interior, item, flow, component, topology, and unrelated external-terminal decision
  left to the solver.

The selected network has one singleton source cell, cell 70, and one singleton demand cell, cell 6.
Both carry one positive flow unit. Root propagation leaves exactly two candidate outgoing arcs at
the source and two candidate incoming arcs at the demand.

For ordered candidates `a_0, ..., a_n`, canonical case `i` posts:

```text
flow(a_i) >= 1
flow(a_j) = 0 for every j < i
```

Later candidates remain free. Therefore each feasible parent solution belongs to the unique case
identified by its first positive candidate. The source and demand partitions are independently
pairwise disjoint and exhaustive; their Cartesian product is an exact partition of the parent
feasible set and preserves every objective value available inside that inherited parent leaf.
Branching, merging, crossings, cycles, and every route-interior choice remain legal. The run is
feasibility-only, so objective quality is preserved structurally but is not optimized or measured.

The preflight proof depends on the current shared-layer semantics:

- the selected terminals are present at root and have singleton cells;
- source and demand cells are distinct;
- selected terminals exclude a bridge at their cell;
- active non-bridge arms in that cell carry the terminal item;
- flows are non-negative;
- flow conservation requires a positive outgoing source arc and positive incoming demand arc.

The experiment fails closed if any premise cannot be certified.

## Controlled Model Change

The selected parent key-24 model contains 63,385 variables, 161,632 constraints, 618,978
incidences, and 242,663 placement-routing incidences. Every child keeps the variable count and
placement-routing incidence count unchanged. It adds exactly two selected-positive flow
constraints plus one unary zero-flow constraint for each earlier candidate:

| Case | Variables | Constraints | Incidences | Placement-routing incidences | Expected added unary constraints |
| ---: | ---: | ---: | ---: | ---: | ---: |
| Parent | 63,385 | 161,632 | 618,978 | 242,663 | 0 |
| 0 | 63,385 | 161,634 | 618,980 | 242,663 | 2 |
| 1 | 63,385 | 161,635 | 618,981 | 242,663 | 3 |
| 2 | 63,385 | 161,635 | 618,981 | 242,663 | 3 |
| 3 | 63,385 | 161,636 | 618,982 | 242,663 | 4 |

The measured deltas match the expected deltas exactly. Authoritative and observation solves also
have identical formulation IDs, complete exact-model metrics, and complexity metrics within each
case.

## Root Propagation Effect

The partition is active but local:

| Root state | Parent | Case 0 | Case 1 | Case 2 | Case 3 |
| --- | ---: | ---: | ---: | ---: | ---: |
| Belt route cells fixed true | 10 | 12 | 12 | 12 | 12 |
| Belt route cells unresolved | 146 | 144 | 144 | 144 | 144 |
| Belt route arcs fixed true | 0 | 2 | 2 | 2 | 2 |
| Belt route arcs unresolved | 508 | 504 | 503 | 503 | 502 |
| Belt flows with positive lower bound | 0 | 2 | 2 | 2 | 2 |
| Belt flows unresolved | 508 | 504 | 503 | 503 | 502 |
| Pipe route arcs unresolved | 508 | 508 | 508 | 508 | 508 |

The two mandatory positive flows propagate to two route arcs and two route cells. Canonical
earlier-zero constraints remove at most two additional belt arcs. The partition leaves roughly five
hundred belt arcs and all five hundred eight pipe arcs unresolved. The first native search decision
also remains the same pipe-bridge rotation Boolean in the parent and every child.

This is the strongest direct explanation for the negative result: the exact split resolves the
selected endpoints' first and last step but barely changes the global routing state on which the
native search begins.

## Interpretation

The controlled endpoint leaf is no longer the best next cliff target. Its complete local choice has
only four cases, all four are measured, and fixing it changes too little of the remaining exact
model.

The next experiment should target a complete semantic family that can explain the remaining global
routing freedom. Candidate families, in evidence order, are:

1. reuse this audited exact partition for the fully internal pipe network
   `network:pipe:item-liquid-xiranite-poly`; its root census has two source continuations and three
   demand continuations, so the complete portfolio has only six cases and targets the layer on
   which native search currently makes its first decision;
2. if all six cases remain unknown, build an exact selected-item route-interior separator cut for
   that pipe network, if a
   complete cut can be derived without choosing a path or corridor; an interior material arc must
   be expressed as `flow > 0 AND item == selected material`, with an exact canonical complement,
   because positive flow alone does not identify its material away from a terminal;
3. use an exact external-pipe-terminal side partition as a control for the five remaining pipe
   external terminals, while recognizing that one terminal cannot resolve their joint geometry
   freedom;
4. investigate a single exact topology enum per cell before partitioning individual bridge
   Booleans; manually splitting the current first bridge literal would mostly duplicate Pumpkin's
   first native branch;
5. run a branching-policy A/B that changes solver search order without changing the model, only after a
   semantically complete domain has been identified.

The next slice must not fix a hand-selected route, corridor, or shortest path. Such a restriction
would be heuristic and would remove the research subject.

## Improvements Preserved in the Exact Baseline

The experiment retains the cumulative exact work:

1. shared belt and pipe physical layers;
2. factored placement and port variables;
3. canonical physical occupancy coupled bidirectionally to transport occupancy;
4. external terminals in the same commodity-network routing model;
5. exact parallel dimension cases and proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, and residual-tuple partitions;
10. exact sparse legal external boundary-key domains;
11. exact external side and cell specialization;
12. semantic endpoint preservation across independently rebuilt width models;
13. root endpoint-continuation census with flow domains and canonical exact partition certificates.

None of these improvements preselects a facility placement, port, route, corridor, or transport
shape in the production exact solver.

## Artifacts

- machine-readable report: `/tmp/aic-phase3-endpoint-continuation.kGFo1q/summary.json`;
- self-contained report: `/tmp/aic-phase3-endpoint-continuation.kGFo1q/summary.html`;
- per-case authoritative and observation wireframes:
  `/tmp/aic-phase3-endpoint-continuation.kGFo1q/case-*.html`.

The total chained diagnostic took 320,298 ms. The new authoritative wave took 5,673 ms, the
observation wave took 5,678 ms, and the endpoint-continuation experiment itself took 11,351 ms.
The long total includes reconstruction of every accepted exact parent partition.

## Independent Review

Three independent post-run reviews examined semantic soundness, experimental isolation, and the
next exact strategy. All three passed the result. They confirmed:

- all four Cartesian-product cases are present exactly once;
- the canonical first-positive construction is disjoint and exhaustive under the actual
  shared-layer terminal, item, and flow-conservation semantics;
- only the expected two to four unary flow restrictions differ from the key-24 parent;
- authoritative and observation models are structurally identical within each case;
- facility, port, boundary-domain, and continuation certificates all pass;
- the result supports only the bounded claim that this local split is not sufficient to cross the
  five-second cliff.

Reviewer cautions are reflected above: concurrent timeout counters are not a speed ranking,
objective quality was preserved but not optimized in this feasibility-only run, and any future
interior split must use an item-specific positive-flow predicate. No unresolved blocker remains.

## Verification

```text
cargo fmt --all -- --check
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
git diff --check
```

All 326 workspace tests passed: 34 main CLI tests, one prior-terminal CLI test, and 291 library
tests. The release benchmark exited successfully.
