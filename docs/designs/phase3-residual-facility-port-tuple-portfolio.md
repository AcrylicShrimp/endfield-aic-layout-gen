# Phase 3 Residual Facility-Port Tuple Portfolio

## Purpose

The selected Phase 3 prior-input pair root snapshot has four remaining non-singleton facility
terminal domains. Each terminal retains two pipe-port values. This experiment removes that complete
remaining facility-port decision class by solving every tuple in the Cartesian product `2^4`.

The experiment asks whether complete facility-port fixation crosses the five-second first-witness
cliff. It does not change placement, external-terminal, routing, item, topology, flow, capacity, or
collision semantics.

## Parent Selection

The parent is derived by rerunning the accepted prior-input pair portfolio and selecting the lowest
`pair_index` whose completed outcome is `Unknown`. The experiment must not use the older
predeclared observation index.

The parent reproduction must satisfy:

- cumulative Heavy Xiranite Phase 3;
- exact used dimensions `16 x 16`;
- the four facility placements fixed exactly as in the parent diagnostic;
- the same introduced-facility coordinate and rotation;
- the same nine inherited terminal assignments and two selected prior-input assignments;
- sparse endpoint support and the accepted exact propagator stack;
- no invalid witness or failed fixed-state assertion.

## Residual Domain Derivation

The root observer must report the actual surviving facility port IDs, not only the declared port
list or cardinality. The portfolio selects every facility terminal whose root port-choice
cardinality exceeds one and requires exactly:

- four distinct facility terminal IDs;
- cardinality two for every selected domain;
- two distinct surviving port IDs for every selected domain;
- no explicitly fixed terminal among the selected domains;
- singleton placement for all four facilities.

Terminal IDs are sorted lexicographically. Surviving port IDs preserve their model-domain order.
The Cartesian product is enumerated in mixed-radix order with a stable zero-based case index. No
tuple is removed in advance. Equal physical-port choices across different terminals remain in the
portfolio and may be proven infeasible by Pumpkin.

## Exact Case Contract

Each of the sixteen cases fixes only the four derived facility port choices in addition to the
parent's eleven assignments. It then runs two separate solves of the same exact `16 x 16` joint
model:

1. an authoritative uninstrumented solve with the standard five-second case budget;
2. a diagnostic root-observation solve with a separately reported budget and outcome.

Only the uninstrumented solve is used for the five-second cliff comparison and authoritative search
counters. Root-census traversal time must not consume that budget.

For every case:

- facility placement and rotation remain the parent diagnostic equalities;
- all ten external terminal choices remain solver decisions;
- belt and pipe cells, arcs, material assignment, flow, topology, bridges, components, and route
  shape remain solver decisions;
- no route, corridor, port-order, or placement heuristic is introduced;
- the separate root observer verifies the fixed ports and their geometry without changing branch
  selection in the authoritative solve;
- success and failure both emit standalone self-contained HTML.

The sixteen cases are a complete disjoint partition of the selected parent's residual
facility-port domains. Some tuples may be empty or proven infeasible after cross-terminal
constraints are applied.

## Report Schema

The machine-readable report records:

- schema version and complete parent root-snapshot report;
- sorted residual terminal domains and surviving port IDs;
- exact tuple count, worker count, authoritative and observation budgets, preparation time, both
  wave wall times, and total time;
- one case row per tuple with assignments, authoritative outcome/layout/timing/search counters/model
  scale, separate observation outcome and root capture status, and fixed-port assertion result;
- two explicitly named aggregate views:
  - authoritative performance counts use only uninstrumented five-second outcomes;
  - combined logical-evidence counts merge each child's authoritative and observation outcomes;
- whether the complete parent is proven infeasible;
- the lowest-index completed unknown child, when one exists;
- interpretation-blocked status.

For every observed child on which the brancher is called, the value-exact fixation assertion
requires:

- exactly fifteen distinct explicitly fixed facility terminal IDs: eleven parent assignments plus
  four tuple assignments;
- each terminal's surviving port domain to be the exact requested port ID, not merely a singleton;
- singleton endpoint geometry matching the requested port on the fixed placement and rotation.

A child proven infeasible during root propagation before the observer is called is a valid proof,
not an assertion failure. Its posted tuple provenance and authoritative/observation outcomes remain
recorded, but no unavailable singleton snapshot is fabricated.

## Outcome Interpretation

Each child's combined logical outcome is computed in this strict order:

1. any invalid witness makes the combined outcome invalid;
2. a validated witness in one solve and a proven-infeasible result in the other is inconsistent
   evidence and blocks interpretation;
3. otherwise any validated witness makes it feasible;
4. otherwise any proven-infeasible result makes it proven infeasible;
5. otherwise it remains unknown.

The five-second cliff comparison uses authoritative outcomes only. Parent proof/witness aggregation,
complete-partition closure, and selection of the lowest-index unknown child use combined logical
outcomes, also preserving the separate parent observation's proof or witness.

After global aggregation, any coexistence of parent-level validated-witness evidence and
complete-parent infeasibility evidence blocks interpretation and records both provenances. The
report must not expose both `parent_witness_found` and `parent_infeasibility_proven` as accepted
conclusions or select a next control from conflicting evidence.

- Any validated witness proves that the selected parent pair is feasible and identifies a complete
  facility-port tuple.
- A parent root-observation witness or any child witness proves the selected parent pair feasible.
- A parent root-observation infeasibility proof or all sixteen child tuples proven infeasible proves
  the selected parent pair infeasible. Sound proof/witness evidence from the parent observation or
  separate child observations must not be lost merely because an authoritative child times out.
- No validated witness plus at least one unknown case shows that complete facility-port fixation
  does not cross the cliff within the case budget, including the all-unknown outcome. The
  lowest-index unknown child becomes the deterministic control for the next exact boundary-key
  support A/B.
- Any invalid witness, missing child, duplicate tuple, wrong-value/non-singleton fixation in an
  observed non-root-infeasible child, or parent assertion failure blocks interpretation.

This result applies to the selected fixed-dimension, fixed-placement parent. It does not prove
global Phase 3 feasibility or infeasibility.

## Verification

- Unit-test exact Cartesian enumeration, stable ordering, and preservation of tuples that may be
  infeasible.
- Unit-test that invalid parent domains and duplicate terminal IDs block execution.
- Compare every case's model scale with the parent apart from the four research equalities and
  read-only observer state.
- Run release-mode reproduction and verify JSON/HTML generation for all outcomes.
- Obtain independent soundness, experiment-interpretation, and next-strategy reviews before
  committing.
