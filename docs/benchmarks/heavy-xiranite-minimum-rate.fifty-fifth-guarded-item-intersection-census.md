# Guarded Item Intersection Opportunity Census

## Question

Pumpkin 0.5 does not reject an unresolved guard in:

```text
selected -> left_item = right_item
```

when the two item domains have no common positive item value but their numeric bounds still overlap.
Item code zero denotes an inactive arm. A selected route arc activates both endpoint arms, and a
selected bridge activates all four axis arms, so zero is not support for either composite guarded
transport relation. The semantic weakness is confirmed by the preceding micro-probe. This
experiment asks the separate operational question: does that missed inference occur often enough
inside the current three-facility Heavy Xiranite cliff to justify another active custom propagator?

## Passive observer

The experiment adds one read-only observer per transport layer. It watches the same exact guarded
relations already present in the model:

- 1,920 directed route-arc item equalities; and
- 1,024 bridge-axis item equalities.

For an unresolved guard, it scans the positive layer item universe and records whether the two live
domains have a common value. It never posts a predicate, removes a domain value, changes a solver
decision, or changes the legal solution set.

The observer uses a deduplicated dirty queue. Backtracking discards pending work from the abandoned
branch instead of rescanning every relation: restoring removed values cannot create a new empty
intersection. This keeps the census from turning every backtrack into a full-grid scan.

The observer is enabled only by the research CLI flag:

```text
--observe-guarded-item-intersections
```

It is not part of the production solver stack.

## Command

```text
target/release/aic-cli research sweep-cumulative-scc-fixed-dimensions \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 2 \
  --worker-count 12 \
  --case-time-limit-ms 5000 \
  --active-local-continuation \
  --observe-guarded-item-intersections \
  --output-dir /tmp/aic-guarded-intersection-observe-v2.AJDJ90
```

Every exact used-dimension candidate received the established five-second budget.

## Growth result

| Run | Phase | Facilities | Executed | Feasible | Infeasible | Unknown | Outer wall | Best area |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Current stack | 0 | 1 | 21 | 8 | 13 | 0 | 2,008 ms | 42 |
| Passive observer | 0 | 1 | 21 | 8 | 13 | 0 | 1,858 ms | 42 |
| Current stack | 1 | 2 | 38 | 6 | 32 | 0 | 6,420 ms | 77 |
| Passive observer | 1 | 2 | 39 | 7 | 32 | 0 | 6,609 ms | 77 |
| Current stack | 2 | 3 | 100 | 0 | 36 | 64 | 39,069 ms | none |
| Passive observer | 2 | 3 | 100 | 0 | 36 | 64 | 38,684 ms | none |

The Phase 2 cliff is unchanged: the same outcome totals remain 36 proven-infeasible and 64
unknown, with no feasible incumbent. Earlier-phase case counts can vary after an incumbent is
reported because concurrent workers may already have started additional candidates. The passive
observer also changes propagation scheduling, so its search counters are not a speed comparison.

## Opportunity census

| Phase | Registered relations per case | Relation checks | Unresolved-guard checks | Disjoint checks | Unique disjoint sum | Maximum in one case |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 2,944 | 157,888 | 16,672 | 3,296 | 902 | 125 |
| 1 | 2,944 | 504,687 | 41,946 | 4,182 | 1,076 | 101 |
| 2 | 2,944 | 3,078,172 | 319,321 | 81,711 | 12,235 | 443 |

The Phase 2 unique count is summed across 100 independent solver instances, so the same logical
relation can be counted in more than one fixed-dimension case. The per-case maximum is 443 of 2,944
relations, approximately 15.0%. Ninety-three cases observe at least one disjoint relation: all 64
unknown cases and 29 of the 36 quick infeasibility proofs.

The 81,711 disjoint observations are approximately 25.6% of the 319,321 unresolved-guard checks.
The 12,235 case-local relation identities split into 120 route arcs and 12,115 bridge axes. The
measured weakness is therefore concentrated in optional bridge selection rather than ordinary
route-arc selection.

## Observer cost

The Phase 2 census itself performs:

- 8,542,856 domain notifications;
- 3,078,172 relation checks; and
- 1,057,114 item-membership checks.

The observer deliberately scans exact sparse domains so it cannot miss an interior domain hole.
It remains a low-priority observer and can miss transient opportunities that another propagator
resolves before it runs. Its wall time and branch counters are not a causal performance comparison,
but the observed opportunity count is already large enough to justify the next experiment.

## Decision

Keep this observer research-only, but promote its exact semantic rule to the next active A/B
experiment. When an unresolved route-arc or bridge guard has no common positive item support, the
guard can soundly be set to false. This preserves every legal solution and does not preselect a
route, port, placement, or component.

The first active version should retain the route-arc and bridge-axis split in its counters. The
expected primary effect is bridge pruning; the experiment must confirm whether those frequent
local deductions reduce Phase 2 first-feasible search rather than merely add propagation work.

## Independent review resolution

Three read-only reviews separately examined proof soundness, measurement cost, and research
priority. The following findings were reproduced locally and accepted before finalization:

- item code zero is not support for the composite selected-arc or selected-bridge rule, even though
  it is support for a bare integer equality;
- filling the dirty set with every relation after each backtrack created an unnecessary full scan;
- a previously drafted guard-true history counter conflated sibling branches and was removed; and
- a previously drafted root counter described only the first historical observation and was
  removed rather than used as root-fixpoint evidence.

The reviews also established a remaining limitation: a low-priority passive observer can miss a
short-lived disjoint state if another propagator resolves the guard before the observer runs. The
census is therefore a lower observation of opportunities in the explored search prefixes, not a
proof of the active propagator's benefit. That benefit remains the next A/B experiment.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test -p aic-data guarded_item_intersection -- --nocapture
cargo test -p aic-cli parses_cumulative_parallel_dimension_sweep -- --nocapture
cargo build --release -p aic-cli
```

Temporary result:

```text
/tmp/aic-guarded-intersection-observe-v2.AJDJ90/summary.json
```
