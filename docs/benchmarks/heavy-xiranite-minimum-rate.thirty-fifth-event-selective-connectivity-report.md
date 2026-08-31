# Heavy Xiranite Minimum-Rate Event-Selective Connectivity Report

## Result

The event-selective predicate-watcher version of the exact possible-graph propagator is rejected.
It preserves semantics and reduces some unnecessary custom propagator executions, but Pumpkin's
predicate-notification overhead and changed scheduling make first-witness search slower than the
existing broad-domain-event propagator.

The existing custom propagator remains the best controlled Phase 2 case:

- baseline: `5.388 s`;
- broad-event possible-graph propagator: `4.385 s`; and
- event-selective predicate propagator: `5.527 s`.

The research-only production boundary is unchanged. Neither custom propagator is enabled by the
normal solver path.

## Native Search Statistics

Exact solve reports now record Pumpkin's native search counters as structured JSON:

- branch decisions;
- backtrack events;
- conflicts;
- learned clauses;
- solver propagator calls; and
- restarts.

Pumpkin 0.5 exposes these values only through its statistics writer. The harness captures that
writer per operating-system thread and pairs it with the public brancher backtrack callback. This
does not change the brancher or conflict resolver.

Pumpkin also prints `numAtomicConstraintsPropagated`, but its 0.5 implementation never increments
the backing counter. The report therefore records `atomic_propagations: null` rather than a
misleading zero.

## Thirty-Second-Budget Comparison

All cases solve the same fixed-placement, fixed-terminal cumulative Phase 2 `12x12` feasibility
problem.

| Metric | Baseline | Broad custom | Event-selective custom |
| --- | ---: | ---: | ---: |
| First witness | 5,388 ms | 4,385 ms | 5,527 ms |
| Branch decisions | 57,939 | 43,226 | 55,442 |
| Backtracks | 15,039 | 11,511 | 14,357 |
| Conflicts | 15,019 | 11,505 | 14,353 |
| Learned clauses | 15,019 | 11,505 | 14,353 |
| Solver propagator calls | 12,297,683 | 7,890,787 | 10,726,908 |
| Restarts | 20 | 6 | 4 |
| Custom executions | 0 | 144,775 | 148,838 |
| Arc scans | 0 | 76,441,200 | 78,586,464 |
| Custom selected-demand conflicts | 0 | 921 | 1,218 |
| Predicate notifications | 0 | 0 | 1,013,224 |

Compared with baseline, the broad custom propagator reduces:

- branch decisions by 25.4%;
- backtracks by 23.5%;
- conflicts and learned clauses by 23.4%;
- solver propagator calls by 35.8%; and
- elapsed first-witness time by 18.6%.

This confirms that the broad custom result is not merely timing noise. Its learned connectivity
conflicts materially shorten the search tree.

The event-selective case registers 11,552 exclusion predicates. It receives more than one million
predicate notifications before its first witness. Although the strict five-second run lowers custom
executions from 144,775 to 134,224, the notification machinery costs more than the saved scans and
changes when low-priority connectivity conflicts enter the search.

## Strict Five-Second Comparison

| Case | Outcome | Search | Decisions | Conflicts | Solver propagator calls |
| --- | --- | ---: | ---: | ---: | ---: |
| Baseline | `unknown` | 5,003 ms | 54,554 | 14,211 | 11,645,474 |
| Broad custom | validated feasible | 4,393 ms | 43,226 | 11,505 | 7,890,787 |
| Event-selective custom | `unknown` | 5,002 ms | 48,189 | 13,061 | 9,681,998 |

Timeouts remain `unknown`; they are not infeasibility claims.

## Interpretation

The experiment separates two costs:

1. **Inference value:** possible-graph conflicts substantially reduce decisions, conflicts, and
   general solver propagation.
2. **Implementation cost:** watching thousands of exact exclusion predicates is more expensive
   than the broad integer-domain subscription it replaces.

The next optimization should therefore retain the broad subscription and reduce work inside each
custom execution. The current implementation allocates an adjacency graph, scans every physical
arc, and constructs the complete absent-arc explanation on every wakeup—even when every demand is
reachable and no reason will be used.

## Recommended Next Exact Slice

Keep the same broad-event scheduling and exact inference, but:

1. store immutable outgoing arc indices once in the propagator;
2. traverse possible arcs directly during reachability instead of rebuilding adjacency;
3. visit only arcs reachable from a possible supply;
4. build the expensive absent-arc explanation only after an unsupported demand is found; and
5. compare identical search counters to verify that the search tree remains unchanged while custom
   CPU work falls.

This optimization cannot exclude a solution or alter the propagator's logical result. Incremental
dynamic-connectivity state and layer-wide multi-material propagation remain later experiments.

## Artifacts

- `heavy-xiranite-phase2-event-selective-connectivity-5s/summary.html`
- `heavy-xiranite-phase2-event-selective-connectivity-5s/summary.json`
- `heavy-xiranite-phase2-event-selective-connectivity-30s/summary.html`
- `heavy-xiranite-phase2-event-selective-connectivity-30s/summary.json`
