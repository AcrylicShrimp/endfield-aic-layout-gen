# Phase 3 Representative Prior-Input Port Controls

## Question

Does either remaining five-value belt-demand port on the old powder-producing facility act as an
outcome separator inside one predeclared source-fixed Phase 3 leaf?

This is a representative diagnostic. It does not cover all 40 residual source leaves and cannot
prove the selected state or global Phase 3 feasible or infeasible.

## Representative Selection

Select the unique source leaf whose already-recorded source report index is 0 before observing the
new control results. This is a deterministic pre-result selection from that report, not a claim
that indices remain stable across future code or data changes, and not the fastest or slowest
measured leaf.

The inherited state contains nine distinct terminal fixations:

- four introduced-facility terminals;
- the two preceding target powder demands;
- the preceding target's singleton pipe demand and belt supply;
- the old powder-producing facility's selected powder supply.

The representative source parent must not be `ProvenInfeasible` or `InvalidWitness`.

## Controlled Domains

Read the old source facility's current cumulative exact port domains. Exclude the already fixed
source terminal and singleton domains. Use the terminal's demand direction, belt transport kind,
and the facility port definitions rather than names alone. The current data must expose exactly two
remaining multi-value input-belt demand terminals, each with five distinct values spanning the
complete domain `input-belt-0` through `input-belt-4`.

Do not identify terminals by hardcoded game-data IDs. Record their stable IDs and exact domains in
the report.

## Two Overlapping Exact Suites

For terminal A, execute all five values while terminal B remains a solver decision. The union of A's
five cases equals the representative parent state.

For terminal B, execute all five values while terminal A remains a solver decision. The union of B's
five cases also equals the same representative parent state.

The suites overlap. Their ten cases are not ten disjoint regions and must not be combined into one
infeasibility proof count. Each suite has its own complete-partition proof and witness aggregate.

Each child fixes only one additional terminal. Facility placement, every other terminal, route cell,
route arc, item value, flow value, topology decision, capacity decision, logistics component,
bridge, and occupancy decision remains unchanged from the representative parent's exact contract.

## Measurements

Record:

- source-stage provenance and representative leaf index/outcome;
- all nine inherited assignments;
- both controlled terminal IDs and complete port domains;
- five child cases per suite with selected port and calculated connection cell;
- construction/search/first-incumbent timing;
- decisions, backtracks, conflicts, learned clauses, propagations, and restarts;
- variables, constraints, incidences, and placement-routing incidences;
- suite-local feasible, infeasible, unknown, and invalid counts;
- suite-local witness and complete-infeasibility flags; and
- separate source-stage, preparation, control-wave, and total wall times.

Emit machine-readable JSON, a self-contained HTML summary, and one standalone HTML result per case.
If a fixed placement sends a declared port outside the exact used dimensions, retain that value in
the complete partition, report its connection cell as out of bounds, and let the exact model prove
or refute the case.

## Interpretation

- Mixed outcomes within a suite identify that terminal value as a useful exact separator for this
  representative leaf only.
- Five proven-infeasible cases prove the representative leaf infeasible.
- A validated child proves the representative leaf feasible, not optimal.
- Five unknown cases mean that terminal alone did not separate outcomes within the five-second
  budget.
- Any invalid witness blocks further interpretation.

Derive the next Cartesian control from proof outcomes only. Any value whose complete single-port
case is `ProvenInfeasible` while the other terminal remains free closes its entire row or column.
Do not remove a value merely because its reported connection cell is absent, and stop interpretation
on an invalid witness. Enumerate every ordered pair of the values not closed by these proofs,
including equal-port pairs, under the same representative parent. For the measured result this is
the complete `4 by 4` residual over values `0` through `3`. Only after that pair remains unresolved
should the diagnostic prioritize root-domain and route/flow/topology observability.
