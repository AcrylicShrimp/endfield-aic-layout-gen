# Heavy Xiranite Endpoint Clearance False-Event Filter

## Result

Skipping endpoint-clearance wakeups for orientation selectors already proven false is exact, but
it is not a universal performance improvement. It changes no variable, constraint, reason,
brancher, feasible assignment, or validator. It does change the order in which High-priority
propagators reach the queue, and that scheduling perturbation helps one phase dramatically while
making the current Phase 30 cliff slower.

The filter therefore remains an explicit research profile and is disabled by default. It is not
adopted as the production search profile.

## Exactness Argument

Each clearance relation loops over independent facility orientations. A false selector removes one
orientation from that loop. Removing an unselected rectangle cannot:

- make another orientation geometrically impossible;
- turn multiple separations of another orientation into one separation;
- justify a coordinate-bound update; or
- create a conflict.

Coordinate-bound notifications always enqueue. A selector event also enqueues whenever its own
orientation is true or unresolved. Multiple literals backed by the same rotation domain are
registered independently, so a domain change still enqueues for every affected surviving
orientation. The propagator stores no reversible search state, and Pumpkin restores deductions on
backtracking.

## Contract And Instrumentation

Schema 8 adds `endpoint_clearance_false_event_filter_enabled` to the search profile and separates
notification counts into:

- coordinate notifications;
- orientation notifications;
- false-orientation notifications skipped;
- notifications that enqueue the propagator.

The CLI switch is `--endpoint-clearance-false-event-filter`. Omitting it reproduces the schema 7
wakeup schedule. Counters remain enabled for every measured run.

## Five-Second Repeated Runs

Each row contains four fresh release-profile processes using the Heavy Xiranite minimum-rate
workload, a caller-supplied 50x50 ceiling, and a five-second search budget. Feasible rows report the
median first-witness search time. Phase 30 rows report median branch progress because all runs end
`unknown` at the hard time limit.

| Priority | Phase | Filter | Outcome | Median search | Median decisions | Median conflicts | Median solver propagations |
|---|---:|---|---|---:|---:|---:|---:|
| High | 23 | Off | feasible 4/4 | 972.5 ms | 67,165 | 3,028 | 7,849,944 |
| High | 23 | On | feasible 4/4 | 250.0 ms | 18,473 | 763 | 2,193,362 |
| High | 24 | Off | feasible 4/4 | 101.0 ms | 7,854 | 150 | 1,083,587 |
| High | 24 | On | feasible 4/4 | 102.0 ms | 7,854 | 150 | 1,081,883 |
| High | 29 | Off | feasible 4/4 | 228.5 ms | 13,332 | 344 | 2,178,513 |
| High | 29 | On | feasible 4/4 | 235.0 ms | 13,332 | 344 | 2,169,416 |
| High | 30 | Off | unknown 4/4 | 5,000 ms | 204,417.0 | 11,581.5 | 42,436,039.5 |
| High | 30 | On | unknown 4/4 | 5,000 ms | 203,885.0 | 11,206.0 | 42,465,892.5 |
| Medium | 23 | Off | feasible 4/4 | 85.5 ms | 6,264 | 225 | 699,834 |
| Medium | 23 | On | feasible 4/4 | 84.0 ms | 6,264 | 225 | 699,834 |
| Medium | 24 | Off | feasible 4/4 | 75.0 ms | 5,339 | 145 | 752,114 |
| Medium | 24 | On | feasible 4/4 | 75.0 ms | 5,339 | 145 | 752,114 |
| Medium | 29 | Off | feasible 4/4 | 631.5 ms | 42,760 | 1,030 | 5,515,529 |
| Medium | 29 | On | feasible 4/4 | 637.0 ms | 42,760 | 1,030 | 5,515,529 |
| Medium | 30 | Off | unknown 4/4 | 5,000 ms | 249,768.5 | 9,356.0 | 38,091,859.0 |
| Medium | 30 | On | unknown 4/4 | 5,000 ms | 249,911.5 | 9,360.5 | 38,112,217.0 |

The High/Phase 23 filter reduces median witness time by 74.3%, decisions by 72.5%, and conflicts by
74.8%. Only 23,852 of 1,930,773 notifications are skipped in that trajectory. Such a small skip
fraction, together with the changed decision tree, indicates that the reduction is driven by when
deductions reach the brancher rather than direct local-work savings alone.

High Phase 24 and Phase 29 preserve decisions and conflicts. Medium Phase 23, 24, and 29 preserve
the complete measured search trajectory. These controls show that the filter is not intrinsically
stronger propagation.

## Phase 30 Sequential Long Confirmation

These runs were executed sequentially to avoid inter-process CPU contention.

| Priority | Filter | Outcome | First witness | Decisions | Conflicts | Solver propagations | Skipped notifications |
|---|---|---|---:|---:|---:|---:|---:|
| High | Off | feasible | 21.973 s | 926,491 | 45,277 | 198,537,975 | 0 |
| High | On | feasible | 24.043 s | 941,397 | 46,499 | 205,801,979 | 3,934,556 |
| Medium | Off | feasible | 24.650 s | 1,169,303 | 48,606 | 207,030,043 | 0 |
| Medium | On | feasible | 24.444 s | 1,169,303 | 48,606 | 207,030,874 | 4,818,104 |

At the actual cliff, High priority becomes 9.4% slower and explores a different trajectory. Medium
priority preserves decisions, conflicts, and learned clauses; its 206 ms elapsed difference is not
treated as a stable speedup. The 831 extra solver propagations and 823 extra custom executions are
negligible relative to approximately 207 million and 147 million respectively.

## Interpretation

The tested notification filter is not the next general solution. The dominant cost remains the
search trajectory created by coupled placement, rotation, port, outside-cell, and clearance
choices. A cheap local wakeup rule can perturb that trajectory sharply, but it does not remove the
Phase 30 cliff.

The useful finding is narrower: High-priority endpoint clearance is schedule-sensitive. Future
work should not stack more local skip rules and assume monotonic improvement. The next experiment
should grow or decompose the Phase 30 exact problem while retaining both scheduling profiles as
controls, then target a semantic propagation gap that reduces the remaining choices rather than
only changing their processing order.

## Verification

- `cargo fmt --all -- --check` passed.
- `git diff --check` passed.
- `cargo check --workspace` passed.
- `cargo test --workspace` passed six ladder CLI tests, 347 `aic-data` tests, and all other
  workspace and documentation tests.
- Exhaustive complete-assignment and reified-baseline differential tests still pass.
- New controlled tests cover filtered false events, surviving true sibling scheduling, and the
  filter-disabled baseline schedule.
- The filter-on WarmStart regression forces a conflict, backtracks, and recovers the supported
  sibling after observing an actual skipped false-orientation event.
- All 68 JSON reports and 68 self-contained HTML files were generated by the release CLI.

## Artifacts

- `heavy-xiranite-bottom-up-clearance-false-event-filter/{high,medium}-{off,on}-phase{23,24,29,30}-run{1,2,3,4}/`
- `heavy-xiranite-bottom-up-clearance-false-event-filter/{high,medium}-{off,on}-phase30-30s/`

## Independent Review Resolution

- The proof and bug review returned **PASS**. It verified the LocalId registration, shared-parent
  watcher behavior, queue coalescing, sibling wakeup, reason preservation, and stateless
  backtracking argument. Its optional filter-on backtracking test was added and passes.
- The implementation and artifact review verified all 68 JSON and 68 HTML reports, schema/profile
  plumbing, notification identities, table arithmetic, CLI help, and workspace verification. It
  initially returned **BLOCK** because this section and `git diff --check` were absent; both
  documentation findings are corrected above. Its wording caution about attributing the Phase 23
  result solely from skip fraction was also accepted and corrected.
- The exact-optimization review returned **PASS**. It confirmed that the paired experiment supports
  keeping the filter default-off and rejected it as a general Phase 30 optimization. It recommends
  an exact 4/16/64-way partition of the three newly introduced facilities' directional rotations
  before implementing a stronger local-key-aware clearance propagator.

No review found a semantic bug. The remaining measurement risk is that each long-run cell has only
one sequential sample; this is sufficient to reject default-on because High also explores a larger
tree, but it is not a latency distribution. The filter-disabled schema 8 profile also performs the
new diagnostic notification classification, so it reproduces the old schedule and semantics but
is not an instruction-for-instruction schema 7 binary.
