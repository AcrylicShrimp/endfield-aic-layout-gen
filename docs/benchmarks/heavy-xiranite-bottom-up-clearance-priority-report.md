# Heavy Xiranite Endpoint Clearance Priority Ablation

## Result

Pumpkin scheduling priority has a large, structure-dependent effect on the exact facility-port
clearance rung. `Medium` reduces Phase 23 median first-witness time from 895.0 milliseconds to 79.0
milliseconds, but it increases Phase 30 first-witness time from 21.994 seconds to 26.594 seconds.

The result is not a change in feasibility or solution quality. Both variants build the same exact
model and use the same propagator. Only its scheduling priority differs. The ablation therefore
identifies propagation order and queue coalescing as a major part of the current cliff, while also
showing that one static priority is not uniformly best.

## Contract

The `facility-ports-propagated` rung records the clearance priority independently in
`search_profile.endpoint_clearance_priority`. `high` and `medium` select the corresponding Pumpkin
propagator priority without changing the rung or formulation identity.

Both variants have:

- the same 2,840 variables and 5,702 constraints at Phase 23;
- the same placement, rotation, port, local endpoint geometry, and clearance decisions;
- the same propagator code, inference reasons, semantic certificate, witness extraction, and
  witness validator;
- no heuristic domain restriction or preselected solver decision.

Rung and formulation are already identical. After deleting only `search_profile` plus timing,
search-statistic, custom-counter, and witness observations, the Phase 23 reports are identical.
Priority may change the deterministic search path under a time limit, but it cannot remove a legal
solution or admit an illegal one.

## Repeated Comparison

All runs use one release binary on the same machine, the Heavy Xiranite minimum-rate workload, the
loose `50 x 50` request ceiling, and a five-second first-witness budget. Each Phase 23, 24, and 30
row contains four fresh processes.

| Phase | Facilities | Endpoints | Priority | Outcome | Search time | Decisions | Conflicts | Solver propagations | Clearance executions |
|---:|---:|---:|---|---|---:|---:|---:|---:|---:|
| 23 | 24 | 71 | High | feasible 4/4 | 883-903 ms, median 895.0 | 67,165 | 3,028 | 7,849,944 | 5,513,393 |
| 23 | 24 | 71 | Medium | feasible 4/4 | 77-83 ms, median 79.0 | 6,264 | 225 | 699,834 | 447,240 |
| 24 | 25 | 75 | High | feasible 4/4 | 93-96 ms, median 93.5 | 7,854 | 150 | 1,083,587 | 786,862 |
| 24 | 25 | 75 | Medium | feasible 4/4 | 71-78 ms, median 74.5 | 5,339 | 145 | 752,114 | 519,493 |
| 30 | 33 | 105 | High | unknown 4/4 | 5,000 ms | 225k-235k | 12.8k-13.4k | 47.3M-49.4M | 35.4M-37.0M |
| 30 | 33 | 105 | Medium | unknown 4/4 | 5,000 ms | 270k-283k | 10.3k-10.9k | 41.5M-43.8M | 28.8M-30.5M |

At Phase 23, Medium performs 90.7% fewer decisions, 92.6% fewer conflicts, 91.1% fewer solver
propagations, and 91.9% fewer clearance executions. Median wall time is 91.17% lower, equivalent to
11.33x throughput. This is a different and much shorter search path, not merely cheaper execution
of the same path. At Phase 24, median wall time is 20.32% lower, equivalent to 1.255x throughput;
the search-path reduction is more modest.

At Phase 30, Medium processes more branch decisions within five seconds while executing fewer
solver and clearance propagations. This demonstrates greater throughput per propagation, but it
does not find a witness within the operational budget.

## Growth Between Phase 24 And Phase 30

The following are single same-binary growth probes. They locate priority crossovers; they are not a
latency distribution.

| Phase | Facilities | Endpoints | High | Medium | Faster priority |
|---:|---:|---:|---:|---:|---|
| 25 | 26 | 79 | 218 ms | 104 ms | Medium |
| 26 | 27 | 83 | 149 ms | 135 ms | Medium |
| 27 | 28 | 87 | 201 ms | 123 ms | Medium |
| 28 | 29 | 91 | 533 ms | 390 ms | Medium |
| 29 | 30 | 95 | 207 ms | 566 ms | High |

The winner flips at Phase 29 in this growth sequence. Facility count alone therefore does not
predict the better priority. The interaction depends on the constraint graph and resulting search
trajectory.

## Phase 30 Long Confirmation

| Priority | Outcome | First witness | Decisions | Conflicts | Solver propagations | Clearance executions | Clearance notifications |
|---|---|---:|---:|---:|---:|---:|---:|
| High | feasible | 21.994 s | 926,491 | 45,277 | 198,537,975 | 150,557,306 | 191,371,017 |
| Medium | feasible | 26.594 s | 1,169,303 | 48,606 | 207,030,043 | 146,921,936 | 228,853,861 |

These are one paired confirmation per priority, not latency percentiles. Medium explores a
different, longer branch path and reaches a witness 4.600 seconds later. The result rejects the
hypothesis that Medium is a universal replacement for High.

## Interpretation

The Phase 23 result explains part of the surprising 24-versus-25-facility inversion. The difficulty
is not proportional to the number of facilities. Adding one facility changes which constraints
wake first, how notifications coalesce, which values Pumpkin branches on, and which conflicts it
learns. A different priority can turn the same Phase 23 model from 67 thousand decisions into six
thousand decisions, while becoming worse again at Phase 29 or 30.

Keep both priorities as reproducible exact experiments. Do not silently replace High with Medium as
a universal default. An exact outer portfolio may run both independent searches and accept the
first validated witness without changing the solution set, but the current Phase 30 evidence shows
that priority diversity alone does not remove the cliff.

## Recommended Next Experiment

First ablate the diagnostic counter cost. The Phase 30 runs perform hundreds of millions of relaxed
atomic counter increments, so instrumentation may materially inflate the measured hot path. Compare
counters enabled and disabled while requiring an identical search trajectory.

Then add the cheapest exact event-source filter: a notification that proves an orientation selector
false can skip enqueue because removing an unselected geometry class cannot create a clearance
inference. Coordinate-bound events and selector-true events must still enqueue. Measure source,
enqueue, and skip counts in the diagnostic build. Do not start with a full actionable-state scan in
`notify`; notifications outnumber executions, so recomputing every orientation mask per notification
could duplicate more work than it removes.

Benchmark the filter under both High and Medium priority. The current result demonstrates that a
scheduling improvement must not be evaluated at only one phase or one priority.

## Verification

- `cargo fmt --all -- --check` passed.
- `git diff --check` passed.
- `cargo check --workspace` passed.
- `cargo test --workspace` passed all workspace and documentation tests, including six
  `aic-bottom-up-ladder` CLI tests and 345 `aic-data` tests.
- The workspace tests include controlled feasible and infeasible facility-clearance cases under
  both priorities and the seven existing endpoint-clearance semantic, differential, deduction, and
  backtracking tests.

## Independent Review Resolution

- The proof and bug review returned **PASS**. It verified exhaustive enum/dispatcher plumbing,
  consistent schema version 6, identical propagator semantics and validation, and tests for both
  feasible and infeasible cases under both priorities.
- The artifact and formulation review initially blocked because the current-build High comparison
  matrix and report were incomplete. After adding the fourth Phase 30 run, fresh High Phase 25-29
  growth, a fresh High 30-second confirmation, and the report, it passed the code, 72 artifacts,
  arithmetic, and no-universal-winner conclusion. Its final documentation-accounting findings were
  then resolved by this section and the explicit command results above.
- The optimization review returned **PASS** for preserving both priorities as exact reproducible
  search profiles and **BLOCK** for replacing High with Medium universally. It also identified the
  original separate-rung representation as an architecture error. The implementation was corrected
  so rung/formulation remain identical and priority is recorded under `search_profile`. Its ranked
  next actions are the counter-cost ablation followed by the cheap selector-false event filter.

No review finding changed game semantics, restricted a solver domain, or introduced a heuristic
layout decision.

## Artifacts

- `heavy-xiranite-bottom-up-clearance-priority/high-phase23-run1/` through `run4/`
- `heavy-xiranite-bottom-up-clearance-priority/medium-phase23-run1/` through `run4/`
- equivalent repeated directories for Phase 24 and Phase 30
- `heavy-xiranite-bottom-up-clearance-priority/high-growth-phase25/` through `phase29/`
- `heavy-xiranite-bottom-up-clearance-priority/medium-growth-phase25/` through `phase29/`
- `heavy-xiranite-bottom-up-clearance-priority/high-phase30-30s/`
- `heavy-xiranite-bottom-up-clearance-priority/medium-phase30-30s/`
