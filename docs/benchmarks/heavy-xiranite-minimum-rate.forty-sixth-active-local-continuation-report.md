# Heavy Xiranite Minimum-Rate Active Local Continuation Report

## Result

The event-driven non-bridge local continuation propagator removes a material portion of the fixed
Phase 2 search tree. It runs beside the accepted watched-demand chain and preserves the exact joint
placement, port, and routing solution space.

| Metric | Watched demand | Active local continuation | Change |
| --- | ---: | ---: | ---: |
| Search time | 3,978 ms | 3,383 ms | -15.0% |
| Branch decisions | 43,543 | 35,338 | -18.8% |
| Backtracks | 8,577 | 7,265 | -15.3% |
| Conflicts | 8,577 | 7,265 | -15.3% |
| Learned clauses | 8,577 | 7,265 | -15.3% |
| Solver propagations | 7,070,786 | 5,602,601 | -20.8% |
| Objective | `(144, 110, 57, 12, 20)` | `(144, 105, 30, 12, 37)` | different first witness |
| Validation | passed | passed | unchanged |

The controlled research search stops at the first feasible witness. The active case's witness is
lexicographically better through the first three configured objectives, but this single run is not
an optimization-quality comparison. It establishes no quality loss and a clear search-work
reduction.

## Active Work

The active propagator records:

- 28,823 executions;
- 358,572 dirty `(material, cell)` evaluations;
- 461,727 relevant notifications;
- at most 720 dirty keys in one pass;
- 1,119 forced predicate attempts;
- three propagator conflicts; and
- at most 15 predicates in one reason.

The passive Phase 45 implementation performed 149,027 material passes, each scanning 144 cells,
or about 21.46 million material/cell checks. The event-driven implementation evaluates 358,572
dirty keys, a 98.3% reduction in local-rule cell checks. The total solver gain shows that the
stronger inference outweighs its remaining notification and queue overhead.

## Exact Contract

At a proven non-bridge cell, definite positive material flow plus no possible opposing terminal and
one possible same-material continuation forces that continuation. Zero continuations negate one
concrete positive witness and therefore conflict when that witness is already selected. Forward and
backward rules are exact duals.

Every reason is rebuilt from live domains and contains:

- one concrete selected terminal, or a selected arc with its two fixed item identities;
- the same-layer bridge exclusion when a bridge variable exists;
- all excluded opposing terminals; and
- one current blocker for every alternative arc.

The zero-support arc-witness reason omits the selected predicate being negated but retains both item
identities. Multiple supports, possible opposing terminals, and bridge-possible cells stop the rule.
Circulation remains legal.

## Event Coverage

The reverse event map is conservative:

- every arc selection/item event dirties both endpoints for every layer material;
- every terminal event dirties its own material and cell; and
- every bridge event dirties every material at that cell.

The initial call checks every key. Later calls drain a deduplicated dirty set. Self-notifications
remain queued for another fixpoint pass, and no domain-derived witness, support, blocker, or reason
survives across a notification or backtrack.

## Verification

Ten controlled fixtures pass in both normal and `pumpkin-debug-checks` modes:

- supply-rooted forward chain;
- demand-rooted backward chain and self-notification;
- two-support branch stopping;
- bridge exclusion wakeup;
- terminal-witness zero-support conflict;
- arc-witness zero-support conflict;
- selected circulation;
- same-material item-loss wakeup;
- reverse fanout through a shared item variable to every material; and
- a deterministic warm-started conflict with at least one real backtrack.

The complete workspace test and format/check suites are recorded at commit time.

## Decision

Accept the event-driven local continuation propagator as a successful exact Phase 2 experiment. Do
not spend the next research slice only micro-optimizing its queue or reason construction. The
propagator has weakened the measured Phase 2 cliff; the next required action is to enable this exact
mode in cumulative SCC growth, grow until the first new timeout or sharp search jump, and decompose
that new blocker before designing another semantic propagator.

Local safe optimizations remain available later: avoid unused reason construction, index terminals
by cell, replace the ordered dirty set with a generation-stamped dense queue, and suppress ordinary
cell events while a bridge remains possible. They are implementation work, not the next blocker
study.

## Artifact

- `heavy-xiranite-phase2-active-local-continuation-10s/summary.json`
- `heavy-xiranite-phase2-active-local-continuation-10s/summary.html`

The release command was:

```bash
target/release/aic-cli research diagnose-phase2-possible-graph-connectivity \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.12x12.request.json \
  --target-phase 2 \
  --used-width 12 \
  --used-height 12 \
  --facility-x 0 \
  --facility-y 1 \
  --port-assignment-index 5 \
  --prefix-case-time-limit-ms 5000 \
  --reference-time-limit-ms 30000 \
  --case-time-limit-ms 10000 \
  --output-dir docs/benchmarks/heavy-xiranite-phase2-active-local-continuation-10s
```
