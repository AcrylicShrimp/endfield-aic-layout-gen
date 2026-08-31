# Heavy Xiranite Minimum-Rate Demand-Silent Connectivity Report

## Result

Omitting demand-selection variables from broad connectivity wakeups is exact but does not produce a
meaningful performance improvement. The variant is rejected as an optimization target.

The registered integer-variable count falls from 14,416 to 11,552, but custom executions, native
search statistics, custom loop counts, validation, and witness objective are all unchanged. The
measured first-witness difference, `3.974 s` versus `3.928 s`, is only 1.2% and is not supported by
any reduction in executed work.

The accepted diagnostic baseline remains broad-event lazy arc traversal with lazy explanations.
The normal solver path remains unchanged.

## Exactness Argument

Demand-domain reductions cannot destroy a possible material path. Only route-arc exclusion,
required-item exclusion, or supply-option exclusion can do so. Selecting a currently reachable
demand changes no path, and a previously unreachable optional demand has already been excluded at
the prior propagator fixpoint.

A controlled test confirms both sides:

- selecting a reachable demand does not execute the demand-silent propagator; and
- excluding its last possible route arc still executes the propagator and reports the conflict.

No solver variable, legal solution, placement, port, terminal, or route decision is removed.

## Five-Second-Budget Comparison

All cases solve the same fixed-placement, fixed-terminal cumulative Phase 2 `12x12` feasibility
problem.

| Metric | Broad lazy | Demand-silent lazy |
| --- | ---: | ---: |
| First witness | 3,974 ms | 3,928 ms |
| Registered domain variables | 14,416 | 11,552 |
| Branch decisions | 43,226 | 43,226 |
| Backtracks | 11,511 | 11,511 |
| Conflicts | 11,505 | 11,505 |
| Learned clauses | 11,505 | 11,505 |
| Solver propagator calls | 7,890,787 | 7,890,787 |
| Custom executions | 144,775 | 144,775 |
| Reachability arc checks | 9,808,005 | 9,808,005 |
| Demand-option checks | 46,758,108 | 46,758,108 |
| Explanation builds | 921 | 921 |
| Custom selected-demand conflicts | 921 | 921 |
| Objective | `144/99/45/12/15` | `144/99/45/12/15` |

Demand-only events evidently coincide with route, item, supply, or other registered domain events
before each connectivity fixpoint. Removing their direct subscriptions therefore does not remove a
single queued custom execution in this workload.

## Three-Iteration Optimization Summary

| Iteration | Exact change | Result | Broad-lazy first witness |
| --- | --- | --- | ---: |
| 1 | Reachable-only arc traversal and lazy explanations | Accepted | 4,561 -> 3,956 ms |
| 2 | Demand options grouped by physical cell | Rejected | 4,201 -> 4,235 ms |
| 3 | Demand variables omitted from wakeup registration | Rejected | 3,974 -> 3,928 ms |

Iteration 1 is meaningful progress: it preserves the complete measured search tree while reducing
custom arc scans by 86.5%, explanation builds by 99.4%, and controlled first-witness time by 13.3%.
Iterations 2 and 3 show that demand-side loop and subscription micro-optimizations have flattened.

The remaining controlled search still performs:

- 43,226 decisions and 11,511 backtracks;
- 7,890,787 native Pumpkin propagator calls;
- 144,775 custom connectivity executions; and
- 46,758,108 demand-option domain queries.

Further progress is unlikely to come from another local list/index tweak. The next research target
should be a stronger exact connectivity inference that reduces the search tree itself, such as
sound mandatory-cut or unique-support propagation for selected demands. That changes inference
strength and explanation design, so it should begin as a separate reviewed experiment rather than
an unbounded continuation of this micro-optimization cycle.

The ideal project target—solving the complete Heavy Xiranite production graph—has not yet been
reached. These experiments improve only the controlled fixed-placement/fixed-terminal cumulative
Phase 2 case and do not enable the custom propagator in the normal solver path.

## Artifact

- `heavy-xiranite-phase2-demand-silent-connectivity-5s/summary.html`
- `heavy-xiranite-phase2-demand-silent-connectivity-5s/summary.json`
