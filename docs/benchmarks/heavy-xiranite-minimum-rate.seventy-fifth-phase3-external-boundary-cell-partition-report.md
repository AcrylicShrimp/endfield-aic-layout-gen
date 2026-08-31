# Heavy Xiranite Phase 3 External Boundary-Cell Partition

## Result

Fixing the selected external final-product demand to one exact north-boundary cell does not cross
the first-feasible cliff. One singleton cell is proven infeasible and the other ten remain
`Unknown` after the five-second search budget:

- key 0: `ProvenInfeasible` after 107 ms;
- keys 24 through 60 in steps of four: `Unknown` after approximately 5,006 ms each;
- validated witnesses: 0;
- invalid witnesses or evidence conflicts: 0;
- interpretation blocked: false.

The solver-certified infeasibility result removes key 0 from the north side of fixed `16 x 16`
residual tuple case 6. Key 0 means the north-facing endpoint at physical cell `(0, 0)`; it does not
prove that the same physical cell is unusable with another direction. The artifact records
Pumpkin's infeasible status but does not contain a separately checked proof file. The result does
not prove the north side or tuple case infeasible. The exact remaining north domain is
`[24, 28, 32, 36, 40, 44, 48, 52, 56, 60]`.

## Why `16 x 16` Was Used

`16 x 16` is not an optimum, a production default, or a game limit. The controlled placement
inherited by this diagnostic requires height 16 and fits from width 13. The earlier experiment kept
the known `16 x 16` canvas so that only one live external-terminal choice changed at a time.

The production orchestration instead enumerates exact dimension cases from sound lower bounds and
shares proven bounds between workers. The Phase 3 failure in that broader path motivated this
controlled diagnosis. Excess canvas remains a plausible cost source: this result neither proves nor
disproves the separate `13 x 16` through `16 x 16` width-sensitivity hypothesis.

## Exact Experiment Contract

The accepted north-side parent is reproduced with:

- exact used dimensions `16 x 16`;
- four facility placements and rotations fixed to the accepted tuple parent;
- all fifteen facility ports fixed;
- ten external boundary terminals present;
- the final-product demand of `network:belt:item-xiranite-enr-powder` selected as the controlled
  terminal;
- every route cell, route arc, item state, flow, topology state, capacity relation, collision
  relation, logistics component, and other external terminal left to the exact solver;
- independent 5,000 ms authoritative and root-observation budgets per child.

The eleven children directly construct the selected boundary selector from singleton sparse
domains. They are non-empty, pairwise disjoint, and exactly cover the accepted north parent. No
placement, port, corridor, or route is selected by a heuristic.

## Exactness and Audit Gates

All gates passed:

| Assertion | Result |
| --- | --- |
| Accepted side parent is unblocked and unresolved | passed |
| Lowest-index unresolved side is north | passed |
| Singleton domains are non-empty, disjoint, sorted, and an exact cover | passed |
| Selected declared/table/routing/restriction certificate domain equals `[key]` | passed |
| Non-selected external certificates match the accepted parent | passed |
| Authoritative and observation certificates agree | passed |
| Selected root domain equals `[key]`, not merely a subset | passed |
| Four facility placements/rotations remain singleton and fixed | passed |
| Fifteen facility-port assignments remain singleton and fixed | passed |
| Child formulations and normalized model metrics satisfy one controlled contract | passed |
| No invalid witness or witness/proof conflict appears | passed |

Every singleton model has exactly 63,385 variables, 161,632 constraints, 618,978 incidences, and
242,663 placement-routing incidences. Each observed selected root domain is exactly its requested
singleton key.

## Measurements

| Key | Outcome | Build ms | Search ms | Decisions | Backtracks | Conflicts | Learned | Propagations |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | ProvenInfeasible | 487 | 107 | 42 | 14 | 15 | 15 | 249,840 |
| 24 | Unknown | 491 | 5,006 | 52,608 | 4,011 | 4,010 | 4,010 | 5,400,198 |
| 28 | Unknown | 488 | 5,006 | 55,904 | 4,106 | 4,105 | 4,105 | 5,342,223 |
| 32 | Unknown | 508 | 5,006 | 56,469 | 3,752 | 3,751 | 3,751 | 5,302,157 |
| 36 | Unknown | 496 | 5,006 | 57,812 | 4,495 | 4,494 | 4,494 | 5,393,226 |
| 40 | Unknown | 557 | 5,006 | 55,106 | 3,946 | 3,945 | 3,945 | 5,424,750 |
| 44 | Unknown | 492 | 5,006 | 52,201 | 4,052 | 4,051 | 4,051 | 5,446,695 |
| 48 | Unknown | 491 | 5,006 | 54,369 | 4,428 | 4,427 | 4,427 | 5,279,782 |
| 52 | Unknown | 484 | 5,006 | 58,243 | 3,821 | 3,820 | 3,820 | 5,287,688 |
| 56 | Unknown | 485 | 5,005 | 54,903 | 4,563 | 4,562 | 4,562 | 5,301,917 |
| 60 | Unknown | 492 | 5,007 | 55,373 | 4,342 | 4,341 | 4,341 | 5,342,268 |

No child produced a first incumbent. The timed-out children execute roughly 52,000 to 58,000
branch decisions and 5.28 to 5.45 million solver propagations. Fixing one boundary cell therefore
removes the endpoint-location disjunction but leaves a large lower routing, item, flow, and topology
search.

The eleven new child experiments took 112,853 ms. Total wall time was 308,537 ms because the
diagnostic CLI reproduced the complete accepted parent chain first; the reproduced side parent
ended at 195,683 ms. This repeated preparation is research-harness overhead, not child search time.

## Interpretation

The side partition established that external boundary geometry matters: west closed quickly. This
singleton refinement now establishes a sharper boundary. The unresolved north cases do not remain
difficult because Pumpkin still has to choose which north cell to use; that choice has already been
reduced to one value before search. The current cliff lies below endpoint geometry, among the
remaining coupled routing, item assignment, flow, capacity, collision, and topology decisions.

Key 0's fast infeasibility result also shows that geometry still provides useful propagation for
some cells. The ten `Unknown` results are evidence of neither feasibility nor infeasibility; they
only show that this propagation is not generally sufficient to construct or refute a route within
five seconds.

The result does not show that a larger canvas is harmless. A `16 x 16` model still contains 256
grid cells and scales the route, arm-item, topology, bridge, and objective state across that canvas.
Width sensitivity is a separate exact experiment and remains available. It must rebuild each
width's semantic boundary domains rather than reusing a raw runtime key from `16 x 16`.

## Next Exact Experiment

Use unresolved key 24 as a representative controlled leaf and run the exact fixed-height width
portfolio `13 x 16`, `14 x 16`, `15 x 16`, and a fresh `16 x 16` replicate. Preserve the semantic
endpoint `(north, x=6, y=0)` by rebuilding and auditing its key for each width. Preserve the same
production graph, four fixed placements/rotations, and fifteen fixed facility ports, and require
every placement to fit before interpreting a case.

This ordering is evidence-driven rather than a response to the earlier canvas-size hypothesis. All
ten unresolved singleton children have the same first semantic decision,
`pipe-bridge-59-rotation-90 <= 0`. Externally splitting that Boolean would mostly repeat the branch
Pumpkin already performs first. In contrast, width changes the replicated grid state that remains
large at root: key 24 still has 295 unresolved route cells, 1,016 unresolved route arcs and flows,
and nine other external selectors with 53 or 54 values.

The widths are neighboring exact fixed-size problems, not a disjoint partition of the `16 x 16`
leaf. A witness proves only its own width fixture feasible, and an infeasibility or `Unknown` result
at one width implies nothing about another. If all widths remain similarly `Unknown`, reject width
as the next cliff breaker and then partition a complete, semantically named domain inside the
route/item/flow/topology chain without fixing a route heuristically.

## Improvements Preserved in the Exact Baseline

The current formulation retains the cumulative exact work:

1. shared belt and pipe physical layers instead of one dense grid per logical line;
2. factored placement and port variables;
3. canonical facility occupancy coupled bidirectionally to transport occupancy;
4. external terminals in the same commodity-network routing model;
5. exact parallel dimension cases with proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, and residual-tuple partitions;
10. exact sparse legal external boundary-key domains, removing impossible root values;
11. exact external side specialization with complete proof aggregation;
12. exact external boundary-cell singleton specialization with root-equality auditing.

## Independent Review

Three independent reviews examined proof soundness, experimental isolation, and next-strategy
selection. Before the release run, one reviewer found that the initial root audit accepted any
subset of `[key]`, including an accidentally empty vector. The cell-specific audit now requires
exact equality for every non-root-infeasible child, with focused exact, empty, wrong, extra, and
root-infeasible tests. The human HTML table was also extended to include learned-clause counts.

Post-run reviewers agreed that endpoint singleton selection did not break the main cliff. They
considered a two-way split of the recorded first bridge-rotation Boolean and a width-sensitivity
control. The selected next step is width sensitivity because the native brancher already chooses
that Boolean first, while width directly changes thousands of unresolved grid-wide states. This is
an experimental ordering decision, not a claim that width is the cause.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```

The final release run produced no invalid witness, evidence conflict, failed fixation, failed
certificate, or blocked interpretation.

## Artifact

The final release artifact is `/tmp/aic-phase3-boundary-cell.GOxXrQ`. It contains:

- `summary.json`, `stdout.json`, and self-contained `summary.html`;
- authoritative and root-observation HTML for every singleton key.
