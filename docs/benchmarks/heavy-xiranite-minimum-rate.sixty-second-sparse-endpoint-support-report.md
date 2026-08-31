# Sparse Endpoint Support Propagator Report

## Result

The exact sparse ternary propagator removes Pumpkin's hidden row-selector variables. It matches the
positive table on every measured projected-domain probe and additionally removes declared values
absent from every legal row. After one bounded implementation optimization, the channel-only cost
is approximately equal to the authoritative nested Element encoding and far below the positive
table.

The faithful Heavy Xiranite Phase 3 joint solve still finds no incumbent in three five-second runs.
The sparse channel changes the explored tree substantially, but it does not cross the current
first-feasible cliff. This closes endpoint-channel implementation tuning for now. The next slice
must diagnose the remaining Phase 3 cliff rather than optimize this propagator again.

## Exact rule

For every facility terminal, the propagator enforces the complete runtime-derived relation

```text
(placement, port, physical endpoint geometry) in legal rows
```

by removing a value exactly when none of its legal rows has both other values still present. One
placement variable remains shared by every terminal of the same facility. Ports remain distinct
even when they alias one geometry. No placement, port, direction, coordinate, or row is selected in
advance.

Every removal reason contains one already-true blocker from every legal row for the target value.
The target predicate is excluded from its own explanation. Residue entries are revalidated against
the current domains before use, so a stale entry after backtracking cannot justify pruning.

## Controlled correctness

The sparse propagator preserves exactly the same complete assignments as the positive table under
exhaustive enumeration of the controlled relation. It also matches the positive-table outcome and
non-conflicting domain fixpoint for all six restrictions:

1. fixed placement and port;
2. interior geometry hole;
3. direction class only;
4. removal of every support for one placement;
5. placement hole propagated forward; and
6. conflict shared through several terminals' common placement.

The actual Phase 3 introduced-facility relation contains 480 shared placement values, four
terminals, and 7,280 legal rows. The sparse propagator matches the positive-table result for every
applicable restriction. Both detect the shared-placement contradiction; their partial domain
snapshots at the point of conflict differ because they reach the contradiction through different
valid propagation orders.

The faithful integrated model starts facility geometry as the dense `0..4 * cell_count - 1`
domain. Corrected sparse support additionally removes every value outside the legal-row projection
at root. That is sound generalized arc consistency stronger than the native positive table's
observed root behavior; the positive-table fixpoint equality claim is intentionally limited to the
common projected probe domains.

The known feasible Phase 0 `7 x 7` joint case is also preserved. Sparse support finds a complete
incumbent in 52 ms, and independent witness validation passes.

## Channel-only implementation cost

Each encoding ran in three isolated release processes. The table reports medians.

| Metric | Nested Element | Positive table | Sparse support |
|---|---:|---:|---:|
| Authored integer variables | 25 | 9 | 9 |
| Hidden row selectors | 0 | 7,280 | 0 |
| Model build | 483 us | 26,962 us | 529 us |
| Maximum RSS | 13,565,952 B | 46,596,096 B | 12,845,056 B |
| Retired instructions | 236,731,113 | 3,846,919,715 | 242,134,277 |
| CPU cycles | 51,917,460 | 783,786,947 | 56,813,923 |

Compared with the positive table, sparse support cuts median build time by 98.0%, RSS by 72.4%,
retired instructions by 93.7%, and cycles by 92.8%. Compared with nested Element, build time is
9.5% higher, retired instructions are 2.3% higher, cycles are 9.4% higher, and RSS is 5.3% lower.
Unlike nested Element, it closes the
measured interior-hole, direction-class, and last-support propagation gaps.

The first draft was materially slower because it built the reverse index by scanning every row for
every distinct value and updated an atomic metric on every support membership test. The final
implementation builds all three reverse indexes in one row pass and batches hot-path counters once
per column propagation. This was the last implementation-only optimization authorized for this
component in the current cycle.

## Faithful Phase 3 joint comparison

The authoritative nested Element stack and sparse-support research stack each ran three isolated
release processes on fixed `16 x 16`, with the standard five-second candidate budget and identical
placement, routing, flow, capacity, topology, occupancy, connectivity, and grid propagators.

| Median metric | Nested Element | Sparse support | Change |
|---|---:|---:|---:|
| Outcome | Unknown, no incumbent | Unknown, no incumbent | No crossing |
| Construction | 476 ms | 466 ms | -2.1% |
| Branch decisions | 3,210 | 3,251 | +1.3% |
| Backtracks | 248 | 460 | +85.5% |
| Conflicts | 247 | 459 | +85.8% |
| Learned clauses | 247 | 459 | +85.8% |
| Solver propagations | 3,563,351 | 2,264,077 | -36.5% |
| Maximum RSS | 455,884,800 B | 476,610,560 B | +4.5% |
| Retired instructions | 102,089,735,456 | 106,651,299,047 | +4.5% |
| CPU cycles | 20,839,838,399 | 20,870,900,584 | +0.1% |

The sparse relation is therefore active and consequential: it reduces solver propagation events by
36.5%, but the explored conflict pattern changes and does not become smaller. It is not a solved
performance improvement: the instrumented sparse-support run retires 4.5% more instructions,
memory grows slightly in the full model, and neither encoding produces a witness. The process-wide
counter comparison is conservative and does not isolate the propagator's intrinsic cost.
The evidence does not justify replacing the authoritative nested Element path yet.

Across the median sparse run, the endpoint propagators execute 23,087 times, check 6,884,985
current values, obtain 6,753,797 valid residue hits, scan 2,220,601 relation rows, remove 87,897
values, and report 17 direct empty-domain conflicts. The 98.1% residue-hit rate shows that most
support checks are residue revalidations; row search remains material. Further event-selective
tuning may reduce that cost, but this report does not
authorize another local tuning cycle before the next cliff is identified.

## Current exact improvements retained

The comparison retains:

- one shared belt layer and one shared pipe layer;
- independent placement and port variables;
- canonical facility occupancy coupled bidirectionally to transport occupancy;
- external inputs and final outputs represented by ordinary shared-network semantics;
- exact fixed-dimension partitioning;
- possible-path connectivity propagation;
- watched unique demand support and local continuation propagation; and
- grouped guarded positive-item intersection propagation.

Sparse endpoint support remains an exact research encoding available for diagnosis. It is not yet
the default endpoint contract.

## Decision and next cliff

Stop optimizing the endpoint propagator. The stronger exact channel did not move first-feasible
status, so endpoint support was a real propagation gap but not the dominant Phase 3 blocker.

The next experiment should rerun the existing Phase 3 residual-state hierarchy with sparse support
enabled, changing no fixation semantics. It should progressively expose complete facility state
and then transport state until one five-second unknown case becomes fast or proves infeasible. The
first transition that collapses search identifies the next coupling cliff under the stronger
channel. Only after that transition is measured should a new semantic propagator be proposed.

The authoritative nested path remains the baseline. Sparse support is the diagnostic variant so
the already-measured endpoint gap does not hide the next blocker.

## Independent review disposition

Three independent read-only reviews passed after two initial blockers were corrected.

- The proof review found that the first propagator indexed only legal-row values and would miss a
  dense-domain value absent from every row. The final constructor receives every declared domain
  value, removes projection-external values with an empty static reason, and has regression tests
  for superset domains and empty relations. Pumpkin debug-check verification passes.
- The experiment review found that the first joint integration bypassed model-complexity recording
  and that its artifacts predated the domain correction. The final integration records one ternary
  `EndpointLink` global constraint per facility terminal: 16 constraints and 48 terms, compared
  with 72 constraints and 200 terms in the nested encoding. Every number in this report comes from
  corrected rebuilt release artifacts. Scaled probe DTOs do not contain a cross-artifact oracle
  match flag; the six cases were compared externally by applicability, conflict status, and
  non-conflicting domain snapshots.
- The follow-up strategy review confirmed the exact semantics and recommended stopping local
  endpoint tuning after this paired matrix. Endpoint geometry to first route cell/arm support is a
  hypothesis for the next propagator, not a measured conclusion. The residual-state hierarchy must
  first identify the transition that collapses the current cliff.

One non-blocking maintenance risk remains: `domain_values` and the corresponding Pumpkin variable
domain are synchronized by crate-internal callers rather than derived by the propagator API. Every
current caller was checked and passes the full domain explicitly.

## Artifacts

- Controlled final run: `/tmp/aic-sparse-endpoint-controlled-final.xWgVbC`
- Channel-only corrected runs: `/tmp/aic-sparse-endpoint-channel-corrected.jf7jyo`
- Joint Phase 3 corrected runs: `/tmp/aic-sparse-endpoint-phase3-corrected.IsX2Ta`
- Validated Phase 0 corrected witness: `/tmp/aic-sparse-endpoint-phase0-corrected.lmYMzy`

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli
git diff --check
```
