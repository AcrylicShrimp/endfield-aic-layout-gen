# Phase 3 Prior-Terminal Completion Portfolio

## Purpose

Close the remaining target-facility port-choice disjunction inside the eight unresolved Phase 3
demand-pair cases. This is a control experiment, not a claim that the remaining output port is the
cause of the cliff.

## Predecessor Evidence

The complete pair-value portfolio partitions the two `item-xiranite-powder` demand terminals into
25 cases:

- 17 cases are proven infeasible while all other preceding ports remain free; and
- 8 cases remain unknown when exactly one demand selects `input-belt-4`.

The same validated preceding-phase placement reference used by that pair portfolio must be retained
for every completion child. A newly solved prefix is not interchangeable evidence even if it happens
to produce the same objective.

## Exact Expansion

The selected preceding target facility has two remaining terminals:

- its pipe demand has one compatible value, `input-pipe-5`; and
- its belt supply has five compatible values, `output-belt-0` through `output-belt-4`.

For each of the eight non-closed parent pairs, the portfolio executes all five remaining output-port
values. The singleton pipe value is fixed and reported but creates no branch. The resulting 40
children fully assign all four terminals on the preceding target facility.

The exact coverage is:

```text
17 proven-infeasible parent regions
union
40 complete child regions under the 8 unresolved parents
```

No legal solution is removed because only parent regions with a proof of infeasibility are not
expanded.

## Inputs

- cumulative Phase 3 at exact used dimensions `16 x 16`;
- the selected introduced-facility coordinate, rotation, and complete port assignment;
- the exact validated preceding-phase reference shared with the parent pair portfolio;
- stable preceding-facility bit 2;
- stable demand terminal bits 2 and 3;
- sparse endpoint-support encoding; and
- five seconds of search per parent and child case.

## Outputs

The report records:

- the complete parent pair stage and its exact preceding reference;
- every remaining terminal domain;
- parent-to-child coverage;
- all 40 complete target-facility port tuples;
- outcome, construction/search/first-incumbent timing, model scale, and native search counters per
  child;
- aggregate feasible, infeasible, unknown, and invalid counts; and
- separate preparation, parent-portfolio, child-portfolio, and total wall times.

The CLI emits JSON, an HTML summary, and one standalone layout HTML file per child automatically.

## Interpretation

- If all 40 children finish quickly, the complete target port tuple was the residual gate.
- If any fully assigned child still times out, that leaf is a routing-only cliff with respect to the
  target facility's port decisions.
- If a child produces a validated witness, that tuple is feasible for the selected placement state;
  this does not prove objective optimality.
- The selected state is proven infeasible only when the 17 closed parents and all 40 children are
  proven infeasible.

If fully assigned children remain unknown, the next smallest port control is the still-free supply
terminal of the old `item-xiranite-powder` source in the same commodity network. Only after port
controls are exhausted should routing topology or capacity propagation be changed.

## Invariants

- Parent regions are skipped only with a solver proof of infeasibility.
- Every completion-domain assignment is executed exactly once for each retained parent.
- The parent and child stages share the exact same preceding reference object.
- Other preceding facility ports remain solver decisions.
- Routing, flow, topology, capacity, items, occupancy, and logistics components remain solver
  decisions.
- No branch-order heuristic or production solver fallback is introduced.
