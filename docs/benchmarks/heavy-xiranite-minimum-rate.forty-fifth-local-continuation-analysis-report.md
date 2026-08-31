# Heavy Xiranite Minimum-Rate Local Continuation Analysis Report

## Result

A passive local-flow analyzer finds a substantial exact inference target beyond the accepted
watched-demand chain. The final release A/B completes with the same search tree and independently
validated witness:

| Metric | Watched demand | Watched demand plus passive analyzer |
| --- | ---: | ---: |
| Search time | 3,712 ms | 8,843 ms |
| Branch decisions | 43,543 | 43,543 |
| Backtracks | 8,577 | 8,577 |
| Conflicts | 8,577 | 8,577 |
| Learned clauses | 8,577 | 8,577 |
| Solver propagations | 7,070,786 | 7,105,473 |
| Objective | `(144, 110, 57, 12, 20)` | `(144, 110, 57, 12, 20)` |
| Validation | passed | passed |

The passive analyzer deliberately performs no propagation. Its added time is measurement overhead,
not an estimate of the future event-driven propagator cost.

## Conservative Opportunity Lower Bound

The analyzer recognizes a definite positive same-material flow witness entering or leaving a cell.
When the opposite local terminal is impossible and only one continuation arc remains possible, cell
conservation makes that arc mandatory. Forward and backward directions are analyzed separately.

The final run records:

- 133 distinct unresolved forward support arcs;
- 94 distinct unresolved backward support arcs;
- 3,356 repeated unresolved forward predicates;
- 2,131 repeated unresolved backward predicates;
- 19 repeated backward zero-support observations; and
- an estimated maximum local explanation size of 15 predicates.

Raw opportunity counters are repeated observations across propagation calls. Only the distinct-arc
counters deduplicate by `(material, selected arc variable)`.

This is a conservative lower bound. The analyzer skips a material/cell observation whenever a
same-layer bridge can still be selected at that cell, because a bridge carries its two axes
independently. It skipped 3,384,462 such observations. Axis-aware bridge continuation is outside this
slice and cannot be used as a pruning premise yet.

## Rejected Preliminary Evidence

The initial 5-second and 10-second artifacts did not receive bridge state. Their Boolean observer
still changed no domains, but their opportunity counts could combine perpendicular bridge axes and
therefore are rejected as proof-readiness evidence. Their rejected measurements are summarized here,
but the schema-incompatible generated files are not part of the accepted artifact set.

The 5-second run also timed out because its broad observer scanned every material and cell on every
registered domain event. The completed final run confirms semantic preservation, while its 34,687
analyzer executions and 149,027 material passes confirm that the broad implementation must not be
promoted to a runtime propagator.

## Exact Proof Boundary

For a non-bridge cell and material `m`, the candidate rule is:

```text
definite positive inflow of m
and no possible local demand for m
and exactly one possible outgoing m arc
=> select that arc and fix both incident item values to m
```

The backward rule is the exact dual. A selected terminal is a one-predicate positive witness. A
selected arc is a three-predicate witness: selected plus both incident item values fixed to `m`.
Every excluded terminal contributes its selection exclusion, and every unavailable alternative arc
contributes one live blocking predicate. A cell with a bridge variable also contributes the proven
`bridge selected = 0` exclusion. Zero-support cases use the same explanation boundary and are
included in the maximum reason estimate.

The rule does not require terminal connectivity and does not reject circulation. It must stop when
an opposing terminal remains possible, multiple continuation arcs remain possible, or a bridge can
still occupy the cell.

## Controlled Verification

Six focused fixtures cover:

- supply-rooted forward continuation without forcing;
- demand-rooted backward continuation without forcing;
- arc-rooted branch and zero-support stopping;
- selected circulation without rejection; and
- conservative exclusion while a same-layer bridge remains possible, including wakeup when that
  bridge becomes impossible; and
- bridge-exclusion reason-size accounting.

Both normal and `pumpkin-debug-checks` test modes pass. Debug mode may invoke the stateless passive
observer more than once, so repeated counters use lower-bound assertions while distinct counters
remain exact sets.

## Review Resolution

Three independent reviews blocked the preliminary implementation. Accepted findings and repairs:

- bridge axes were incorrectly aggregated: bridge-possible cells are now registered and skipped;
- debug checker calls repeated the diagnostic scan: fixtures now distinguish repeated observations
  from distinct opportunities;
- zero-support observations were omitted from the explanation maximum: they now contribute;
- a timeout could not demonstrate preservation: the final 10-second A/B completes identically; and
- broad rescanning is too expensive: the implementation remains diagnostic-only.

No reviewer found a soundness defect in the non-bridge forward/backward Boolean implication itself.
Active propagation remains a separate next slice and requires another proof review.

## Improvements Reached Before This Checkpoint

The propagator research sequence has now established these cumulative improvements:

1. one shared directed grid per transport layer replaced per-network dense route grids;
2. facility placement and port assignment were factored into independent exact solver state;
3. shared physical occupancy strengthened facility/transport propagation;
4. external connections became ordinary shared-network routes instead of shape-forced connectors;
5. exact dimension partitioning moved bounding-box choice outside individual solver instances;
6. terminal support and recursive unique-support chain propagators reduced the Phase 2 search tree;
7. reverse watched-demand scheduling cut redundant chain executions by 32.1% and chain steps by
   66.8%; and
8. this slice identifies at least 133 forward and 94 backward residual support arcs for the next
   stronger exact rule.

## Decision

Accept the passive diagnostic and its non-bridge proof boundary. Do not enable its broad scanner in
normal solving. The next exact experiment should implement the same forward/backward rule with dirty
`(material, cell)` scheduling, live-domain reasons, self-notification handling, and backtrack-safe
tests. Success must be judged by fewer decisions or backtracks, not by forced-predicate count alone.

## Artifacts

- `heavy-xiranite-phase2-local-continuation-non-bridge-10s/`: accepted conservative A/B.

The final release command was:

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
  --output-dir docs/benchmarks/heavy-xiranite-phase2-local-continuation-non-bridge-10s
```
