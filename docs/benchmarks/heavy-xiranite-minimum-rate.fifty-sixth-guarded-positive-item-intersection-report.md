# Active Guarded Positive-Item Intersection Report

## Question

The preceding passive census found frequent states where an unresolved route-arc or bridge guard
had no common positive item support across its guarded item equality. This experiment asks whether
the exact inference

```text
no positive item remains in both endpoint domains -> selection guard = false
```

reduces the current three-facility Heavy Xiranite Phase 2 cliff.

## Exact rule

The active propagator watches the two item domains of every directed route arc and each bridge
axis. A selected route arc activates both endpoint arms. A selected bridge activates every arm on
both axes. The arm-item table permits item code zero only when an arm is inactive. Therefore every
selected relation requires a common item in `1..=layer_item_count`.

For every positive item code, an empty intersection reason contains one currently true
disequality from whichever side excludes that code. The reason has at most one predicate per
layer item. Posting `guard <= 0` is consequently semantics-preserving and cannot remove a legal
layout.

The propagator is research-only and enabled by:

```text
--active-local-continuation --active-guarded-item-intersections
```

It does not fix placement, port, route, bridge, or item decisions.

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
  --active-guarded-item-intersections \
  --output-dir /tmp/aic-guarded-intersection-active-fixed.m5l1JP
```

The baseline is the same release-mode local-continuation stack without the final active flag:

```text
/tmp/aic-guard-clause-16x16-5s.DgSYZn/summary.json
```

Every executed dimension case receives an independent five-second search budget. Model
construction is outside that budget.

## Operational growth result

| Run | Phase | Facilities | Executed | Feasible | Infeasible | Unknown | Outer wall |
|---|---:|---:|---:|---:|---:|---:|---:|
| Baseline | 0 | 1 | 21 | 8 | 13 | 0 | 2,008 ms |
| Active | 0 | 1 | 20 | 7 | 13 | 0 | 1,866 ms |
| Baseline | 1 | 2 | 38 | 6 | 32 | 0 | 6,420 ms |
| Active | 1 | 2 | 36 | 4 | 32 | 0 | 7,460 ms |
| Baseline | 2 | 3 | 100 | 0 | 36 | 64 | 39,069 ms |
| Active | 2 | 3 | 100 | 0 | 39 | 61 | 37,852 ms |

The earlier-phase executed and feasible counts are scheduling-sensitive. Once a parallel worker
reports an incumbent, candidates above its area can be skipped while already-running workers still
finish. The cumulative harness can also carry a different non-binding prior hint into the next
phase. Those rows are operational end-to-end results, not paired causal comparisons.

Phase 2 executes the same complete 100-candidate set in both runs because neither stack finds an
incumbent. The active stack proves three cases that timed out under the baseline:

| Dimensions | Baseline | Active | Active search | Guard rejection attempts |
|---|---|---|---:|---:|
| 12x9 | unknown at 5 s | proven infeasible | 1,231 ms | 330 |
| 13x9 | unknown at 5 s | proven infeasible | 1,755 ms | 366 |
| 14x9 | unknown at 5 s | proven infeasible | 2,273 ms | 380 |

No Phase 2 case produces a feasible incumbent. The propagator improves the current cliff but does
not cross it.

## Phase 2 search counters

| Metric | Baseline | Active | Change |
|---|---:|---:|---:|
| Branch decisions | 369,019 | 330,852 | -10.3% |
| Backtracks | 26,382 | 21,556 | -18.3% |
| Conflicts | 26,356 | 21,538 | -18.3% |
| Learned clauses | 26,356 | 21,538 | -18.3% |
| Solver propagations | 187,951,797 | 165,936,571 | -11.7% |
| Summed construction time | 64,688 ms | 64,249 ms | -0.7% |
| Summed search time | 369,070 ms | 339,704 ms | -8.0% |

The reduced search counters and three additional proofs show that the inference removes useful
subtrees. They do not establish a general speed ratio: the cumulative runs may use different
Phase 1 hints, and fixed wall-clock cutoffs expose different search prefixes.

## Active propagator work

Across the 100 Phase 2 cases, the active propagator records:

- 2,944 guarded relations and 2,048 distinct watched item domains per case;
- 213,893 executions;
- 3,501,194 item-domain notifications;
- 649,356 guarded relation checks;
- 164,383 guard-rejection attempts;
- 146 direct conflicts while posting a rejection;
- 1,467 route-arc rejection attempts;
- 162,916 bridge rejection attempts; and
- at most 5 reason predicates.

`forced_guard_rejections` counts attempts, including the 146 posts that immediately conflict.
Successful posts are therefore `164,237`. The bridge/route split is also attempt history, not a
count of globally unique geometry. The concentration remains overwhelming: approximately 99% of
attempts target bridge guards.

The passive and active modes share one diagnostic DTO. Passive-only historical-unique fields are
zero in an active report; the top-level observation/propagation mode flags disambiguate the data.
The existing factor-graph recorder does not include custom propagator watcher incidences, so its
generic constraint count must not be interpreted as the complete active runtime structure. The
dedicated registered-relation and watched-domain counters are authoritative for this rule.

## Independent review resolution

Three independent read-only reviews covered proof soundness, Pumpkin integration and cost, and
experimental attribution.

Accepted and fixed findings:

- an initial `mem::take` loop could lose unprocessed dirty relations after a conflict; the active
  loop now retains its tail until backtrack synchronization resets it;
- a two-relation search regression proves that both alternative conflict branches remain visible;
- notification callbacks now skip enqueueing when they add no new dirty relation;
- notifications for a relation whose guard is already false are ignored exactly; and
- debug-mode repeated scratch propagation made an exact diagnostic-counter assertion invalid; the
  test now checks the semantic lower condition instead.

The corrected targeted suite passes with Pumpkin debug reason checking. Reviewers found no legal
solution pruning or unsound explanation. Remaining non-blocking gaps are an independent runtime
checker, an integrated route-and-bridge equivalence fixture, and a causally paired Phase 2 run with
one frozen prior hint.

An earlier active sweep executed before the conflict-tail fix is discarded and is not used in any
table or conclusion.

## Decision and next exact optimization

Retain the active positive-item intersection rule in the measured research stack. It is the first
new rule in this cycle to convert multiple Phase 2 timeouts into proofs while reducing decisions,
conflicts, and native solver propagations.

The next slice should optimize the confirmed bridge-dominated implementation without changing its
semantics:

1. represent both axes of one bridge as one grouped guarded relation;
2. exclude bridge relations whose guard is already statically false;
3. cache a live positive support per relation; and
4. batch hot diagnostic counters outside inner scans.

The grouped relation is especially well targeted: it halves the 1,024 bridge-axis relation records
to 512 bridge guards per 16x16 case, and either disjoint axis is sufficient to reject that guard.
After this cost cut, rerun the same Phase 2 sweep before moving to the endpoint `Element` channel.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo test -p aic-data --features pumpkin-debug-checks guarded_item_intersection_propagator -- --nocapture
cargo test -p aic-cli parses_active_guarded_item_intersection_sweep -- --nocapture
cargo build --release -p aic-cli
git diff --check
```
