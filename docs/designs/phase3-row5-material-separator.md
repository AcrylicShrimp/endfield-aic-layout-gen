# Phase 3 Row-5 Material Separator

## Purpose

Run one final route-local exact diagnostic inside material-junction child E. The experiment asks
where the selected liquid-xiranite material first crosses from row 5 to row 6 after the inherited
prefix:

```text
48 -> 64 -> 80 -> 81
```

This is a diagnostic of E only. Junction sibling S remains unresolved and no result from this
experiment may close the full junction parent unless S is resolved separately.

## Accepted Parent

The harness must fail closed unless all inherited reports and raw certificates establish:

- target phase 3;
- exact dimensions `16 x 16` for this controlled fixture;
- selected network `network:pipe:item-liquid-xiranite-poly` and selected item code 5;
- source terminal at cell 48 and demand terminal at cell 113;
- endpoint source continuation `48 -> 64` with demand continuation unrestricted;
- row-4 separator case 0, `Q(64 -> 80)`;
- material-junction child E, `Q(80 -> 81)`;
- authoritative and observation evidence agree and neither contains an invalid witness;
- every inherited model/certificate/root-domain gate passes.

`Q(e)` means:

```text
selected(e) = 1 AND from_item(e) = 5
```

## Exact Partition

The complete row-5 separator contains all 16 south-directed arcs:

```text
80 -> 96, 81 -> 97, ..., 95 -> 111
```

For child `i`:

```text
Q(candidate[i])
AND not Q(candidate[0])
AND ...
AND not Q(candidate[i - 1])
```

The selected source is on or above row 5, the selected demand is below row 5, and all modeled grid
movement is orthogonal. Any selected-material source-to-demand flow must cross this complete cut at
least once. The canonical first-true children are therefore non-empty as predicates, pairwise
disjoint, and exhaustive over E.

The partition must not assume from visual intuition that only x=0 or x=1 can occur. All 16
candidates are modeled unless the unrestricted E control root proves a candidate predicate false
and the artifact records that proof. The initial implementation enumerates all 16.

Later crossings, northward recrossings, multiple crossings, splitters, convergers, cycles, bridges,
flow magnitudes, facility placement, rotation, ports, every downstream route, and every other
commodity network remain solver decisions.

## Exact Encoding

Reuse the native material-separator predicate encoding:

- selected child predicate: two unary constraints;
- preceding predicate exclusion: `selected = 0 OR from_item != 5`;
- no selector variables;
- no route, placement, port, corridor, or topology heuristic.

The exact model must post both inherited and new separator restrictions:

1. row 4 selected case 0;
2. row 5 control or selected canonical child.

The endpoint encoding therefore accepts an ordered list of material-separator restrictions. The
root snapshot and raw certificate schemas expose the same ordered list. This is a hard internal
cutover from the prior optional singleton field; obsolete singleton plumbing is removed.

The harness requires list length 2 and exact order `[row4, row5]`. The row-4 entry is always selected
case 0. The row-5 entry is `None` in the control and `Some(i)` in child `i`. A missing, duplicate,
reordered, or extra separator blocks interpretation. Authoritative and observation ordered
certificate lists and ordered root snapshots must agree. The junction E certificate must remain
unchanged across the control and all children.

The material-junction E restriction remains a separate semantic rule. The model posts all three
accepted restrictions independently:

```text
source continuation
row-4 separator
junction E
row-5 separator
```

## Controlled Comparison

Run one unrestricted row-5 control and all 16 canonical children under independent solver instances.
Use the accepted default of five seconds per authoritative case and five seconds per observation
case. Respect `worker_count` for each wave and sort results by case index before comparison.

Execution is strictly staged: authoritative control, complete authoritative child wave,
observation control, then complete observation child wave. Authoritative and observation instances
must never overlap. Each 16-child wave executes in `worker_count`-bounded chunks, and the report
records execution order and wave wall time.

The control and children must share:

- identical production graph, fixed fixture, fixed inherited decisions, solver mode, time budget,
  objective state, hint, propagators, and model construction path;
- identical boundary, endpoint-continuation, row-4 separator, and junction certificates;
- identical variable count and placement-routing incidence count.

Only the new row-5 separator clauses may differ. Child `i` must add exactly `2 + i` constraints and
`2 + 2i` incidences relative to the row-5 control.

The control raw certificate must establish all exact-cover premises directly:

- width and height exactly `16 x 16` and separator row exactly 5;
- candidate list exactly the ordered 16 arcs `80 + x -> 96 + x`, with indices `0..15`, no duplicate
  or omitted arc;
- source and source-continuation rows at or above 5 and demand row below 5;
- the exact selected network, item code, balanced positive source/demand pair, and terminal cells;
- native route, flow, and arm-item variable families with expected declared bounds.

Authoritative and observation raw certificates must match. These checks derive exact cover from the
actual constructed model rather than trusting a reported candidate count.

The model-delta gate compares the complete constraint-family vector. The control adds no row-5
constraint or incidence beyond the inherited row-4 separator and junction. Child `i` adds exactly
`2 + i` constraints and `2 + 2i` incidences, all in `material-separator`; every other family count
and incidence count is unchanged. Variable count, placement-routing incidence count, hidden-domain
coverage, exact model identity apart from the row-5 control/partition label, and all inherited
certificate lists remain equal.

## Diagnostics And Artifacts

The machine-readable report includes:

- parent junction report and explicit selected parent child E;
- row-5 candidate list and exact-cover proof premises;
- authoritative and observation outcome for every case;
- construction time, search time, first-incumbent time, termination reason;
- branch decisions, backtracks, conflicts, learned clauses, and solver propagations;
- variables, constraints, incidences, and placement-routing incidences;
- ordered raw separator certificates for row 4 and row 5;
- root snapshots for both separators and the junction;
- model-family deltas and hidden-domain coverage;
- evidence conflicts, invalid witnesses, and every fail-closed gate;
- total parent reconstruction and local experiment wall time.

For a root-infeasible child, static certificates, ordered-list identity, model-family deltas, and all
exact-cover premises remain mandatory. Dynamic root-domain comparisons are recorded as unavailable,
not as observed or silently passed. A root proof is accepted only when the authoritative and
observation evidence rules permit `ProvenInfeasible` and every applicable static gate passes.

Emit JSON, a self-contained HTML summary, and authoritative/observation wireframes for the control
and all children regardless of success, proof, or timeout.

## Interpretation Rules

- A validated witness proves only that its row-5 child in E is feasible.
- A proven-infeasible child removes only that canonical row-5 region from E.
- All row-5 children proven infeasible close E, but S and the junction parent remain unresolved.
- `Unknown` means only that the case was unresolved at the cutoff.
- Timeout counter differences are descriptive and cannot rank children or prove a causal blocker.
- A root-forced first child with root-conflicting siblings means the row-5 partition is redundant.

## Stop Rule

This is the final planned cell-by-cell route refinement.

- If a witness appears, stop and report it.
- If exact proofs materially close E, preserve them and stop before automatically descending to row
  6.
- If the remaining children again consume the full budget without a witness or substantial proof,
  stop route-local splitting.
- If the row-5 control already root-forces one canonical child, treat the split as redundant and stop
  route-local splitting.
- Any certificate, model-isolation, proof, or authoritative-observation conflict blocks
  interpretation.

After the stop, the next design investigation changes scale. It compares exact cross-network/shared-
topology decomposition and an iterative exact master/subproblem architecture in which routing
failures return sound cuts to placement/port/topology decisions. No such architecture replaces the
approved joint baseline without a separate contract, measurement, and explicit user approval.
