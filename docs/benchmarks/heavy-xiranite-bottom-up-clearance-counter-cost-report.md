# Heavy Xiranite Endpoint Clearance Counter-Cost Ablation

## Result

Endpoint-clearance diagnostic counters are not a material part of the current cliff. Disabling
every relaxed atomic update preserves the exact search trajectory and changes the Phase 30
first-witness time by only 0.5% under High priority and 0.1% under Medium priority.

This is a negative but useful result. The next optimization should target unnecessary propagator
wakeups, not remove diagnostic evidence.

## Contract

The `facility-ports-propagated` search profile now records
`endpoint_clearance_counters_enabled`. When false, the propagator skips only diagnostic atomic
updates. It retains the same event registrations, priority, propagation logic, reasons, variables,
constraints, feasible set, search brancher, and witness validator.

Counter-disabled reports intentionally contain zero-valued endpoint-clearance statistics. Solver
search statistics remain available and are the primary equivalence evidence.

## Short Repeated Runs

Each row contains four fresh release-profile processes under the same workload, request ceiling,
and five-second budget.

| Priority | Phase | Counters | Outcome | Search range | Median search | Decisions | Conflicts | Solver propagations |
|---|---:|---|---|---:|---:|---:|---:|---:|
| High | 23 | On | feasible 4/4 | 863-876 ms | 868.0 ms | 67,165 | 3,028 | 7,849,944 |
| High | 23 | Off | feasible 4/4 | 864-899 ms | 880.5 ms | 67,165 | 3,028 | 7,849,944 |
| Medium | 23 | On | feasible 4/4 | 75-76 ms | 75.5 ms | 6,264 | 225 | 699,834 |
| Medium | 23 | Off | feasible 4/4 | 75 ms | 75.0 ms | 6,264 | 225 | 699,834 |
| High | 24 | On | feasible 4/4 | 90-93 ms | 91.5 ms | 7,854 | 150 | 1,083,587 |
| High | 24 | Off | feasible 4/4 | 90-96 ms | 90.0 ms | 7,854 | 150 | 1,083,587 |
| Medium | 24 | On | feasible 4/4 | 67-70 ms | 69.0 ms | 5,339 | 145 | 752,114 |
| Medium | 24 | Off | feasible 4/4 | 67-72 ms | 68.5 ms | 5,339 | 145 | 752,114 |

Every on/off pair has identical decisions, conflicts, learned clauses, and solver propagations.
Elapsed differences are small, inconsistent in direction, and within process-level timing noise.

## Phase 30 Five-Second Throughput

All 16 runs return `unknown`, as expected. Median branch progress changes by only 0.31% under High
priority and 0.29% under Medium priority.

| Priority | Counters | Decisions range | Median decisions | Conflicts range | Solver propagations range |
|---|---|---:|---:|---:|---:|
| High | On | 239,466-242,885 | 241,446.5 | 13,711-13,889 | 50.48M-51.24M |
| High | Off | 239,290-244,318 | 242,187.5 | 13,705-13,965 | 50.44M-51.56M |
| Medium | On | 294,130-300,728 | 298,484.5 | 11,461-11,826 | 45.70M-46.84M |
| Medium | Off | 296,732-302,608 | 299,348.5 | 11,606-11,902 | 46.15M-47.16M |

## Phase 30 Long Confirmation

| Priority | Counters | Outcome | First witness | Decisions | Conflicts | Solver propagations |
|---|---|---|---:|---:|---:|---:|
| High | On | feasible | 21.042 s | 926,491 | 45,277 | 198,537,975 |
| High | Off | feasible | 20.938 s | 926,491 | 45,277 | 198,537,975 |
| Medium | On | feasible | 24.326 s | 1,169,303 | 48,606 | 207,030,043 |
| Medium | Off | feasible | 24.302 s | 1,169,303 | 48,606 | 207,030,043 |

The exact trajectory identity makes this a direct implementation-cost comparison. Disabling
counters saves 104 milliseconds under High priority and 24 milliseconds under Medium priority.
Those reductions are operationally negligible relative to the 21-24 second cliff.

Each on/off value is one paired long confirmation, not a latency distribution. The repeated
five-second waves support the negative result, but the 0.5% and 0.1% witness-time differences are
not presented as percentiles or stable universal speedups.

## Interpretation

The large raw counter values were misleading as a cost predictor. Relaxed atomic increments on this
single-threaded execution path are cheap enough that removing them does not change the bottleneck.
Counters should remain enabled by default because they provide much more diagnostic value than
their measured cost.

## Recommended Next Experiment

Add the cheapest semantics-preserving event filter. Orientation selector registrations have local
IDs distinct from coordinate bounds. When a selector notification proves that orientation false,
return `Skip`: removing an unselected rectangle geometry cannot create a new rejection, unique
separation, or bound deduction. Continue to enqueue every coordinate-bound event and every selector
event that does not prove false.

Measure notification sources, skipped-false events, enqueue count, executions, and search results
under both High and Medium priority. Do not yet scan all orientations inside `notify`.

## Verification

- `cargo fmt --all -- --check` passed.
- `git diff --check` passed.
- `cargo check --workspace` passed.
- `cargo test --workspace` passed all workspace and documentation tests, including six ladder CLI
  tests and 345 `aic-data` tests.
- The focused facility-port tests cover enabled and disabled profile plumbing. Disabled counters
  return zero statistics while producing a validated witness.

## Independent Review Resolution

- The proof and bug review returned **PASS**. It verified that every atomic update is behind the
  immutable instrumentation flag while propagation, reasons, events, priority, and search remain
  outside it. It also confirmed default-on and zero-snapshot behavior.
- The artifact and formulation review passed the code, schema 7 profile contract, all 104
  artifacts, arithmetic, and negative conclusion. It initially blocked this report on missing
  concrete verification accounting, review dispositions, and the single-pair long-run caveat;
  those documentation findings were corrected above.
- The optimization review returned **PASS** and accepted the counter-cost hypothesis as rejected for
  this measured cliff, without generalizing to all hardware or future instrumentation. It also
  proved selector-false notification skipping exact and recommended it as the next isolated
  experiment under both priorities.

No review changed solver semantics or introduced a heuristic restriction.

## Artifacts

- `heavy-xiranite-bottom-up-clearance-counter-cost/{high,medium}-{on,off}-phase23-run1/` through
  `run4/`
- equivalent Phase 24 and Phase 30 repeated directories
- `heavy-xiranite-bottom-up-clearance-counter-cost/{high,medium}-{on,off}-phase30-30s/`
