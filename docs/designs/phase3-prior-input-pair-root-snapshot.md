# Phase 3 Prior-Input Pair Root Snapshot

## Purpose

The residual prior-input pair portfolio leaves twelve physically distinct input-port pairs
unresolved after five seconds each. This diagnostic observes one deterministic unresolved case at
the root of search to identify which endpoint, placement, and routing domains remain broad before
Pumpkin makes its first branch decision.

The observation case is derived by scanning the completed cases, filtering outcome `Unknown`, and
choosing the minimum `pair_index`. The existing predeclared observation index is not used because
its case was root-proven-infeasible. The derived pair index and baseline outcome are recorded. This
case is not a representative proof sample, and no conclusion from it may be generalized to the
other eleven unresolved pairs without a later controlled comparison.

## Exact Model Contract

The observed solve reproduces the selected pair case without changing its feasible set:

- cumulative Heavy Xiranite Phase 3;
- exact used dimensions `16 x 16`;
- the same introduced-facility coordinate, rotation, and fixed port assignment;
- the same validated Phase 2 reference;
- the same preceding-facility placement fixation;
- the same nine inherited terminal assignments and two selected prior-input assignments;
- sparse endpoint support, possible-graph reachability, watched-demand support, and local positive
  flow continuation;
- feasibility-only search with the standard five-second case budget.

All four facility placements are fixed in this leaf: the three Phase 2 placements come from the
exact reference and the introduced facility has a fixed coordinate and rotation. Their root domains
are still measured as reproduction assertions. Every unfixed port, external boundary terminal,
route cell, route arc, item domain, flow, topology component, capacity, and collision remains a
solver decision.

## Read-Only Observation Point

The snapshot is captured exactly once, when Pumpkin requests the first branch decision after root
propagation. The observer reads domains from `SelectionContext`, delegates to the unchanged dynamic
brancher, and records the returned first predicate. It must not:

- post a constraint;
- remove a domain value;
- add a hint;
- select or reorder a branch decision;
- change event subscriptions;
- alter conflict analysis or termination.

The observed run is separate from the previously recorded uninstrumented pair result. Its elapsed
time is diagnostic only and is not a performance comparison.

## Snapshot Schema

The machine-readable report records:

1. provenance: derived pair index, both assignments, baseline outcome, capture point, and one of
   `root-infeasible`, `root-solved-without-decision`, or `captured-before-first-decision`;
2. total Pumpkin domains, registered semantic domains, unregistered internal/helper domains, and
   every registered semantic variable family: total, fixed, unresolved, exact
   bounded-cardinality histogram, and lower/upper span histogram;
3. every facility: placement-choice cardinality and the distinct possible `x`, `y`, and rotation
   values implied by its surviving placement candidates; all four must be singleton;
4. every logical terminal: network, transport, direction, endpoint kind, endpoint-geometry domain
   cardinality, facility port-choice cardinality where applicable, and possible/fixed routing-option
   counts. Facility terminals also record declared, surviving, and excluded port counts. External
   terminals keep the raw parent-key cardinality as propagation evidence but derive routable
   boundary sides and cells only from option literals that can still be true;
5. every physical transport layer: true/false/unresolved route-cell and route-arc counts, the same
   counts split into boundary and interior regions, arm-item domain-cardinality histogram, and flow
   counts plus lower/upper width histogram;
6. every material network: possible supply and demand option counts, material-capable possible arc
   count, and demand-option reachability from at least one possible supply. A material-capable arc
   requires `selected` to contain true, both endpoint arm-item domains to contain the material code,
   and the arc flow upper bound to be positive;
7. the first delegated decision: domain identifier, registered semantic family/name when known,
   predicate text, and root domain cardinality;
8. fixed-state assertions: the eleven explicitly fixed facility terminals must have singleton port
   and geometry domains; all remaining non-singleton facility and external terminal domains are
   listed rather than silently classified.

Exact cardinality is computed by enumerating each bounded semantic domain with `contains`. Pumpkin
0.5's `get_size_of_domain` is not used because it reports `upper - lower` and ignores holes.

## Failure Modes

- The portfolio has no `Unknown` pair: return structured invalid input because there is no selected
  unresolved case to observe.
- The selected pair cannot be reproduced: return the exact solver failure and no fabricated
  snapshot.
- The observer is never called because root propagation proves infeasibility: report
  `root-infeasible` with no first decision.
- The delegated brancher returns no predicate because root propagation already completed a
  solution: report `root-solved-without-decision` and validate the resulting witness normally.
- An explicitly fixed facility terminal is not singleton at root: retain the raw snapshot and mark
  the fixed-state assertion as failed.
- The observed solve produces an invalid witness: retain both the snapshot and invalid-witness
  diagnostics and block interpretation.

## Interpretation Boundary

This slice diagnoses the residual domain structure at `16 x 16`. Root broadness and the first
predicate identify candidates for a controlled exact decomposition; neither proves a runtime
bottleneck. A candidate is confirmed only if a later five-second child portfolio changes the cliff.

Registered family counts cover the explicit model and named predicate variables recorded by the
model builder, not every helper domain allocated internally by Pumpkin. The report records this
coverage gap explicitly. Family rankings and an unregistered first decision must not be presented
as a complete census of the solver's active domains.

The snapshot does not establish that `16 x 16` is necessary or best. A later size-sensitivity probe
may reproduce the same fixed logical and placement state at another legal exact dimension. Such a
probe compares different feasible sets and boundary contracts, not performance on an identical
problem.

## Result-To-Experiment Routing

- A non-singleton fixed placement or fixed terminal blocks interpretation and triggers a channeling
  correctness diagnostic.
- An unreachable possible demand without a root conflict triggers possible-graph propagator
  registration and wake analysis.
- Fixed placements with four remaining binary facility pipe-port domains trigger the smallest
  complete exact decomposition first: a `2^4` port portfolio that removes the entire remaining
  facility-port decision class without changing the `16 x 16` problem.
- A broad raw external parent key containing values absent from the legal boundary-key relation
  triggers a separately controlled exact unary-support channel comparison after the facility-port
  portfolio. A one-terminal option partition is not enough to classify the other external
  terminals.
- Narrow endpoints with overwhelmingly unresolved route, arm-item, or route-arc families trigger a
  routing/item propagation diagnosis.
- Narrow route topology with broad flow bounds triggers a flow conservation and capacity diagnosis.
- Dominant branch-component, bridge, or crossing families trigger a topology encoding breakdown.

The first decision family is supporting evidence only. No branch-order change follows from this
snapshot without a separately contracted experiment.
