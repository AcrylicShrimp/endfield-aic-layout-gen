# Guarded Item Predicate Clause Experiment

## Change

The shared-layer model previously encoded every terminal material requirement as two reified linear
inequalities implementing:

```text
selected -> arm_item = required_item
```

The new exact encoding is one predicate clause:

```text
not selected OR arm_item == required_item
```

The legal solution set is unchanged. Placement, rotation, port, route, item, flow, topology, and
capacity remain solver decisions. The new clause is stronger before `selected` is fixed because an
interior removal of `required_item` immediately makes the equality predicate false and unit
propagates `selected=false`.

## Experiment

Both revisions ran the same release command:

```text
target/release/aic-cli research sweep-cumulative-scc-fixed-dimensions \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --worker-count 12 \
  --case-time-limit-ms 5000 \
  --active-local-continuation \
  --output-dir <temporary-directory>
```

The baseline was commit `97811f4`. Every exact used-dimension candidate received the established
five-second budget.

## Growth result

| Encoding | Phase | Facilities | Executed | Feasible | Infeasible | Unknown | Outer wall | Best area |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Reified bounds | 0 | 1 | 21 | 8 | 13 | 0 | 2,521 ms | 42 |
| Predicate clause | 0 | 1 | 21 | 8 | 13 | 0 | 2,008 ms | 42 |
| Reified bounds | 1 | 2 | 37 | 4 | 32 | 1 | 7,670 ms | 77 |
| Predicate clause | 1 | 2 | 38 | 6 | 32 | 0 | 6,420 ms | 77 |
| Reified bounds | 2 | 3 | 100 | 0 | 36 | 64 | 38,525 ms | none |
| Predicate clause | 2 | 3 | 100 | 0 | 36 | 64 | 39,069 ms | none |

The stronger clause removes the Phase 1 timeout but does not move the operational Phase 2 cliff.
Both Phase 2 runs prove the same 36 candidates infeasible and leave the same 64 candidates unknown.

## Aggregate search counters

| Encoding | Phase | Decisions | Backtracks | Conflicts | Solver propagations |
|---|---:|---:|---:|---:|---:|
| Reified bounds | 0 | 16,940 | 3,502 | 3,515 | 8,368,517 |
| Predicate clause | 0 | 12,242 | 2,431 | 2,444 | 6,688,392 |
| Reified bounds | 1 | 14,538 | 2,574 | 2,605 | 22,000,636 |
| Predicate clause | 1 | 27,631 | 4,566 | 4,598 | 23,052,727 |
| Reified bounds | 2 | 349,934 | 24,565 | 24,540 | 211,464,331 |
| Predicate clause | 2 | 369,019 | 26,382 | 26,356 | 187,951,797 |

The Phase 2 clause model executes about 11.1% fewer solver propagations, but performs 5.5% more
branch decisions and 7.4% more conflicts. The per-candidate effect is not uniform. For example, the
12x12 candidate drops from 6,928 to 3,030 decisions, while the 16x16 candidate rises from 6,186 to
7,882. This is consistent with stronger local filtering changing the default brancher's later
decision order rather than eliminating the dominant global disjunction.

## Model cost

For every Phase 2 candidate, the recorder changes from 44,468 to 56,388 total variables while the
recorded 151,715 constraints and 568,985 incidences remain unchanged. The 11,920 additional recorded
Boolean variables equal the Phase 2 facility and boundary endpoint-key option count.

These are explicit equality predicate views required by the clause. Pumpkin's preceding positive
item tables already create predicate and row literals internally, but the generic model recorder
does not observe those hidden allocations. The count increase therefore improves visibility and may
also include newly materialized predicate views; it must not be interpreted as 11,920 wholly new
semantic decisions. Representative Phase 2 model construction increases from roughly 500--532 ms
to 581--652 ms.

## Conclusion

The predicate clause is an exact and demonstrably stronger formulation. It improves Phase 0 and
Phase 1 wall time and removes one Phase 1 timeout. It also reduces aggregate Phase 2 propagation
work, so the cutover is retained.

It is not the Phase 2 cliff breaker. The next audited item path is guarded binary equality:

```text
arc_or_bridge_selected -> left_arm_item = right_arm_item
```

There are 2,944 such guards in the Phase 3 16x16 model. Pumpkin propagates holes exactly after the
guard becomes true, but while the guard is unknown it rejects the guard only when the two domains'
bounds are disjoint. The next experiment must detect empty item-domain intersection without
building an unmeasured large tuple table.

## Temporary artifacts

- baseline: `/tmp/aic-guard-baseline-16x16-5s.5XbfLc/summary.json`
- predicate clause: `/tmp/aic-guard-clause-16x16-5s.DgSYZn/summary.json`

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```
