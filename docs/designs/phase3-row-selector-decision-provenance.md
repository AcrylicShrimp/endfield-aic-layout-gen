# Phase 3 Row-Selector Decision Provenance

## Status

Accepted diagnostic contract. This is a research-only observation slice. It does not change the
authoritative endpoint encoding, branching policy, legal solution set, or objective.

## Question

Does Pumpkin spend a material share of the Phase 3 positive-table search branching on the 29,568
hidden Boolean row selectors created by the standard table encoding?

The hypothesis is supported when row-selector decisions account for a large share of all branch
decisions or form long consecutive runs. It is rejected when row-selector decisions are rare despite
most selectors remaining unresolved after root propagation.

## Controlled model

- Workload: minimum-rate Heavy Xiranite.
- Cumulative phase: Phase 3, four facilities.
- Exact used dimensions: `16 x 16`.
- Search mode: feasibility only.
- Per-run search budget: 5,000 ms.
- Prior hint: none.
- Solver stack: possible-graph reachability, watched demand/local continuation, and grouped guarded
  item-intersection propagation.
- Endpoint semantics: the same complete positive-table relation used in the preceding experiment.

The tracked poster must reproduce Pumpkin 0.5's non-reified positive-table clauses exactly: one
Boolean selector per legal row, row-to-column implications, column-value-to-support clauses, and one
at-least-one-row clause. It may retain the generated selector domain IDs for observation.

## Search invariants

- Construct Pumpkin's normal default brancher over every domain after model posting.
- Do not remove, defer, reorder, prefer, phase, or fix row-selector decisions.
- The observer may inspect the predicate returned by the unchanged inner brancher.
- Event counting must not create clauses, domains, propagators, or solver assignments.
- A timeout remains `unknown`; it is not infeasibility.

## Output

Extend structured search statistics with optional research fields:

- total, root-fixed, and root-unfixed row selectors;
- row-selector and non-row branch decisions;
- selector decisions that set a row true or false;
- selector predicates with an unclassified polarity;
- maximum consecutive row-selector decision run; and
- row-selector predicate appearances during conflict analysis.

Normal solves emit `null` for these fields. The tracked research case emits values even when it
times out without an incumbent. JSON, summary HTML, and layout/failure HTML are written
automatically.

## Correctness checks

1. Compare the native and tracked table encodings exhaustively on the existing small endpoint
   relation.
2. Solve the known-feasible Phase 0 `7 x 7` case with the tracked encoding and require independent
   witness validation to pass.
3. Compare Phase 3 native-table and tracked-table aggregate search statistics. Instrumentation may
   add small wall-time overhead, but it must not intentionally alter the brancher or legal model.

## Decision gate

- Row decisions dominate: the next exact formulation target is a sparse semantic endpoint-support
  propagator without one branchable literal per row.
- Row decisions do not dominate: reject branching dilution and measure clause/support-processing
  cost or return to the residual routing/flow hierarchy.

No custom endpoint propagator or branch-order experiment is implemented in this slice.
