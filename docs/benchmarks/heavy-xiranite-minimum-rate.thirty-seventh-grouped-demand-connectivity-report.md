# Heavy Xiranite Minimum-Rate Grouped Demand Connectivity Report

## Result

The demand-options-by-cell variant is rejected.

It preserves exact semantics, the complete broad-lazy search tree, and the validated witness, but
it does not reduce first-witness time. Grouping skips only 6.56 million individual demand-option
queries while adding 19.90 million demand-cell reachability checks.

The accepted diagnostic baseline remains broad-event lazy arc traversal with lazy explanations.
No custom connectivity propagator is enabled in the normal solver path.

## Five-Second-Budget Comparison

All cases solve the same fixed-placement, fixed-terminal cumulative Phase 2 `12x12` feasibility
problem.

| Metric | Broad eager | Broad lazy | Grouped demands |
| --- | ---: | ---: | ---: |
| First witness | 4,771 ms | 4,201 ms | 4,235 ms |
| Branch decisions | 43,226 | 43,226 | 43,226 |
| Backtracks | 11,511 | 11,511 | 11,511 |
| Conflicts | 11,505 | 11,505 | 11,505 |
| Learned clauses | 11,505 | 11,505 | 11,505 |
| Solver propagator calls | 7,890,787 | 7,890,787 | 7,890,787 |
| Custom executions | 144,775 | 144,775 | 144,775 |
| Reachability arc checks | 76,441,200 | 9,808,005 | 9,808,005 |
| Demand-cell checks | 0 | 0 | 19,897,775 |
| Demand-option checks | 46,639,898 | 46,758,108 | 40,200,900 |
| Explanation builds | 144,775 | 921 | 921 |
| Custom selected-demand conflicts | 921 | 921 | 921 |
| Objective | `144/99/45/12/15` | `144/99/45/12/15` | `144/99/45/12/15` |

The demand-option counter now records actual domain queries rather than multiplying the complete
option list by the number of executions. The lazy case can inspect a prefix twice on the rare 921
unsupported-demand executions: once to discover the first unsupported option and again while
posting all exclusions. This accounts for its small difference from the earlier aggregate count.

## Interpretation

Demand options are not duplicated densely enough by physical cell for a separate cell-level loop
to pay for itself. The grouped representation reduces option-domain queries by 14.0%, but the new
cell loop more than triples the number of cheap demand-side checks. Runtime is effectively flat and
slightly worse in this run.

This result also exposes a more direct exact optimization. The broad propagator currently subscribes
to demand-selection variables even though a demand-domain reduction cannot remove a possible route:

- excluding a demand only removes work;
- selecting a reachable demand leaves the possible graph unchanged; and
- an unreachable demand is already excluded by the prior propagator fixpoint.

Only route-arc, route-item, and supply-option domain reductions can destroy a possible path. The next
experiment should therefore keep broad Pumpkin domain events for graph and supply variables but
omit demand-only variables from the propagator registration. This is not the rejected predicate
watcher design: it uses the same cheap integer-domain event mechanism and changes only the registered
variable set.

## Artifact

- `heavy-xiranite-phase2-grouped-demand-connectivity-5s/summary.html`
- `heavy-xiranite-phase2-grouped-demand-connectivity-5s/summary.json`
