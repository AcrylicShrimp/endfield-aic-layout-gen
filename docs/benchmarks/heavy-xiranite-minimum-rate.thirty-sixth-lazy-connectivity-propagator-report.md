# Heavy Xiranite Minimum-Rate Lazy Connectivity Propagator Report

## Result

The broad-event lazy-traversal possible-graph propagator is accepted as the new diagnostic
connectivity baseline.

It preserves the broad propagator's exact search tree and validated witness while reducing the
work performed inside each custom propagation. On the controlled cumulative Phase 2 `12x12`
case, first-witness time falls from `4.561 s` to `3.956 s`, a 13.3% reduction. Compared with the
unchanged formulation without the custom connectivity inference, it is 32.7% faster.

The research-only production boundary is unchanged. The custom propagator is still disabled in
the normal solver path.

## Exact Equivalence Evidence

The broad eager and broad lazy cases have the same:

- 23,972 solver variables;
- 82,656 recorded constraints;
- eight custom connectivity propagators;
- branch decisions: 43,226;
- backtracks: 11,511;
- conflicts and learned clauses: 11,505;
- solver propagator calls: 7,890,787;
- restarts: 6;
- custom executions: 144,775;
- custom demand pruning attempts and selected-demand conflicts: 921; and
- validated witness objective: `144 / 99 / 45 / 12 / 15`.

The change therefore does not preselect placement, port, terminal, or routing decisions. It does
not alter the possible graph, wakeup schedule, learned connectivity predicates, or legal solution
set. It only changes how the same inference is computed.

## Thirty-Second-Budget Comparison

All cases solve the same fixed-placement, fixed-terminal cumulative Phase 2 `12x12` feasibility
problem.

| Metric | Baseline | Broad eager | Event-selective eager | Broad lazy |
| --- | ---: | ---: | ---: | ---: |
| First witness | 5,880 ms | 4,561 ms | 5,753 ms | 3,956 ms |
| Branch decisions | 57,939 | 43,226 | 55,442 | 43,226 |
| Backtracks | 15,039 | 11,511 | 14,357 | 11,511 |
| Conflicts | 15,019 | 11,505 | 14,353 | 11,505 |
| Learned clauses | 15,019 | 11,505 | 14,353 | 11,505 |
| Solver propagator calls | 12,297,683 | 7,890,787 | 10,726,908 | 7,890,787 |
| Custom executions | 0 | 144,775 | 148,838 | 144,775 |
| Total custom arc scans | 0 | 76,441,200 | 78,586,464 | 10,294,293 |
| Reachability arc checks | 0 | 76,441,200 | 78,586,464 | 9,808,005 |
| Explanation builds | 0 | 144,775 | 148,838 | 921 |
| Explanation arc scans | 0 | 76,441,200 | 78,586,464 | 486,288 |
| Demand-option checks | 0 | 46,762,456 | 47,528,448 | 46,762,456 |
| Custom selected-demand conflicts | 0 | 921 | 1,218 | 921 |

Relative to broad eager propagation, the lazy implementation reduces:

- total custom arc scans by 86.5%;
- reachability arc checks by 87.2%;
- explanation builds by 99.4%;
- explanation arc scans by 99.4%; and
- elapsed first-witness time by 13.3%.

The smaller elapsed improvement than the loop-count improvement is expected. Native Pumpkin work
is unchanged, and the propagator still scans all demand options on every broad wakeup.

## Strict Five-Second Comparison

| Case | Outcome | Search | Decisions | Conflicts | Solver propagator calls |
| --- | --- | ---: | ---: | ---: | ---: |
| Baseline | `unknown` | 5,002 ms | 52,913 | 13,759 | 11,232,317 |
| Broad eager | validated feasible | 4,495 ms | 43,226 | 11,505 | 7,890,787 |
| Event-selective eager | `unknown` | 5,002 ms | 47,207 | 12,802 | 9,491,944 |
| Broad lazy | validated feasible | 3,905 ms | 43,226 | 11,505 | 7,890,787 |

Timeouts remain `unknown`; they are not infeasibility claims.

## Implementation

Each material-layer propagator now builds immutable outgoing-arc indices once. During propagation,
it starts from possible supplies and visits only arcs leaving reached cells. If every selectable
demand remains reachable, it returns without constructing a reason. If a demand is unsupported,
it scans all arcs once and constructs the same eager absent-arc and absent-supply reason used by the
previous implementation.

## Next Measured Bottleneck

The dominant remaining custom loop is no longer arc traversal. The lazy case performs 46,762,456
demand-option checks, about 4.8 times its 9,808,005 reachability arc checks. Every broad wakeup scans
all demand options even though most demand cells are reachable and no pruning occurs in 143,854 of
144,775 executions.

The next exact experiment should group demand options by physical cell and test reachable demand
cells before visiting the options attached to them. This is an internal representation change:
every option at an unreachable cell must still be pruned with the same reason, and no demand,
placement, port, or route choice may be removed in advance. The acceptance criterion remains an
identical broad-lazy search tree with fewer demand-option inspections and lower elapsed time.

## Artifacts

- `heavy-xiranite-phase2-lazy-connectivity-5s/summary.html`
- `heavy-xiranite-phase2-lazy-connectivity-5s/summary.json`
- `heavy-xiranite-phase2-lazy-connectivity-30s/summary.html`
- `heavy-xiranite-phase2-lazy-connectivity-30s/summary.json`
