# Grouped Guarded Positive-Item Intersection Report

## Question

The active positive-item intersection rule improved the three-facility Heavy Xiranite Phase 2
cliff, but approximately 99% of its rejection attempts targeted bridges. The first implementation
represented the two axes of every bridge as two relation records even though both records rejected
the same bridge guard.

This experiment asks whether grouping both bridge axes under one guard and omitting bridge guards
that are already logically impossible on the search-grid perimeter can reduce implementation cost
without changing the exact joint placement-routing problem.

## Exact reformulation

The passive observer remains axis-based so its historical census is unchanged. The active rule now
uses:

```text
route relation  = guard + one opposite-end item pair
bridge relation = guard + horizontal item pair + vertical item pair
```

A bridge remains possible only while both independent axes have at least one common positive item.
The axes may use different items. If either axis becomes positive-domain disjoint, that axis's
disequality reason alone proves that the shared bridge guard must be false.

Only interior grid cells receive an active bridge relation. A bridge on the search-grid perimeter
cannot be selected in the native exact model: a missing axis directly disables it, or selection
would require incoming and outgoing arcs on the sole available side and contradict the existing
opposing-arm constraint. This removes only redundant propagation records, not legal layouts.

If a degenerate grid produces no active relation, the redundant propagator is not installed. No
placement, rotation, port, route, item, flow, component, or dimension decision is restricted.

## Command

```text
/usr/bin/time -l target/release/aic-cli research sweep-cumulative-scc-fixed-dimensions \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 2 \
  --worker-count 12 \
  --case-time-limit-ms 5000 \
  --active-local-continuation \
  --active-guarded-item-intersections \
  --output-dir /tmp/aic-guarded-intersection-grouped-final.uEK8Td
```

Comparisons:

```text
ungrouped active: /tmp/aic-guarded-intersection-active-fixed.m5l1JP/summary.json
no-rule baseline: /tmp/aic-guard-clause-16x16-5s.DgSYZn/summary.json
```

Every dimension candidate receives an independent five-second search budget. Model construction is
outside that budget.

## Structural result

For one 16x16 case across the belt and pipe layers:

| Active structure | Ungrouped | Grouped | Change |
|---|---:|---:|---:|
| Registered relations | 2,944 | 2,312 | -632 (-21.5%) |
| Registered item domains | 2,048 | 1,920 | -128 (-6.25%) |

The grouped count is 1,920 directed route relations plus 392 interior bridge relations. The smaller
domain reduction is expected because adjacent route relations already watch many bridge-axis item
domains.

## Operational growth result

| Phase | Facilities | Executed | Feasible | Infeasible | Unknown | Outer wall |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 1 | 21 | 8 | 13 | 0 | 2,258 ms |
| 1 | 2 | 36 | 4 | 32 | 0 | 5,449 ms |
| 2 | 3 | 100 | 0 | 39 | 61 | 36,886 ms |

The Phase 2 outcome is unchanged from the ungrouped active run. No feasible incumbent appears. The
grouped implementation retains all three proofs that the original baseline could not finish at
12x9, 13x9, and 14x9.

The whole 12-worker process reported 5,197,627,392 bytes (4.84 GiB) maximum resident set size. No
equivalent RSS capture exists for the ungrouped run, so this is an absolute observation rather than
a memory-improvement claim.

## Completed-case comparison

The three newly proven cases traverse the same search tree before and after grouping: branch
decisions, backtracks, and conflicts are identical. They provide the cleanest timing comparison in
the cumulative harness.

| Dimensions | Metric | Ungrouped | Grouped | Change |
|---|---|---:|---:|---:|
| 12x9 | Construction | 619 ms | 575 ms | -7.1% |
| 12x9 | Search | 1,231 ms | 1,197 ms | -2.8% |
| 12x9 | Relation checks | 3,274 | 2,638 | -19.4% |
| 12x9 | Notifications | 10,776 | 10,488 | -2.7% |
| 13x9 | Construction | 581 ms | 538 ms | -7.4% |
| 13x9 | Search | 1,755 ms | 1,648 ms | -6.1% |
| 13x9 | Relation checks | 3,310 | 2,674 | -19.2% |
| 13x9 | Notifications | 12,282 | 12,000 | -2.3% |
| 14x9 | Construction | 537 ms | 518 ms | -3.5% |
| 14x9 | Search | 2,273 ms | 2,169 ms | -4.6% |
| 14x9 | Relation checks | 3,324 | 2,688 | -19.1% |
| 14x9 | Notifications | 13,694 | 13,418 | -2.0% |

Native solver propagations differ by only 37 in each case, confirming that this slice primarily
removes redundant custom-propagator work rather than changing the search proof.

## Aggregate Phase 2 counters

| Metric | Ungrouped | Grouped | Change |
|---|---:|---:|---:|
| Branch decisions | 330,852 | 344,760 | +4.2% |
| Backtracks | 21,556 | 23,138 | +7.3% |
| Conflicts | 21,538 | 23,118 | +7.3% |
| Learned clauses | 21,538 | 23,118 | +7.3% |
| Solver propagations | 165,936,571 | 171,715,843 | +3.5% |
| Summed construction | 64,249 ms | 60,265 ms | -6.2% |
| Summed search | 339,704 ms | 335,585 ms | -1.2% |
| Outer wall | 37,852 ms | 36,886 ms | -2.6% |
| Guarded executions | 213,893 | 218,544 | +2.2% |
| Item-domain notifications | 3,501,194 | 3,523,751 | +0.6% |
| Relation checks | 649,356 | 581,504 | -10.4% |
| Membership checks | 1,966,581 | 2,153,554 | +9.5% |
| Guard-rejection attempts | 164,383 | 161,660 | -1.7% |

Aggregate search-prefix counters are not a causal speed comparison. Earlier phases may choose
different non-binding prior hints, and a fixed timeout lets a faster implementation explore a
longer prefix. The stable completed-case table is stronger evidence. The aggregate data still
confirms that construction and relation-check cost fell while the Phase 2 cliff remained.

## Independent review resolution

Three independent read-only reviews separately examined proof soundness, Pumpkin integration and
cost, and experimental attribution. All returned PASS after these accepted fixes:

- an empty active relation vector now omits propagator construction;
- the propagator name and counters are captured before the base arguments move;
- symmetric horizontal and vertical support-loss wakeup tests are present;
- a selected bridge with a disjoint axis is tested as an immediate conflict; and
- perimeter-cell classification is tested on 1x1 and 3x3 grids.

One non-blocking invariant remains implicit: private generators create only `RouteArc + one pair`
and `Bridge + two pairs`. A future public constructor should encode that shape in its type.

## Decision and next exact experiment

Keep the grouped active relation. It is exact and measurably cheaper, but it is not a Phase 2 cliff
breaker.

The next isolated implementation experiment should remove reason construction from the dominant
supported path. The current scan builds a provisional disequality conjunction even when it later
finds common support and discards the conjunction. First check positive support without allocating;
only a disjoint pair should receive a second pass that constructs the proof reason. Measure this
separately before support caching or diagnostic-counter batching.

If implementation-only cuts stop producing useful gains, the next semantic formulation target is
the weak factored placement-port-geometry `Element` channel identified in the capability audit.

## Verification

```text
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo test -p aic-data guarded_item_intersection_propagator -- --nocapture
cargo test -p aic-data only_interior_grid_cells_have_four_neighbors -- --nocapture
cargo test -p aic-data --features pumpkin-debug-checks guarded_item_intersection_propagator -- --nocapture
cargo build --release -p aic-cli
git diff --check
```

The final workspace run passed 30 CLI tests and 244 library tests. Both focused propagator modes
passed 13 tests.
