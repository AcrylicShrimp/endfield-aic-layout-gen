# Heavy Xiranite Endpoint Clearance Hot-Path Report

## Result

Replacing the per-execution direction `Vec` with a four-bit mask and reading relation bounds once
per propagator execution reduces wall-clock time by approximately 10-13% without changing the
model, search path, or witness. Phase 30 reaches the same first witness in 21.417 seconds instead of
23.711 seconds.

This is an implementation-only optimization of the exact point-versus-variable-rectangle
propagator. It does not add, remove, or restrict any placement, rotation, port, endpoint, or
clearance decision.

## Change

The previous implementation performed two avoidable operations in the hottest exact propagator:

1. every checked orientation allocated a `Vec` with capacity four to record supported separation
   directions;
2. every checked orientation read the same eight coordinate bounds from the solver.

The optimized implementation represents the four directions with one `u8` mask and snapshots the
eight relation bounds once at the start of an execution. A bound posted during that execution can
make the snapshot weaker than the current solver state for later orientations, but it cannot make
an inference unsound. The registered bound event schedules another execution and restores the
normal propagator fixpoint.

## Comparison

All measurements use release-profile builds on the same machine with the Heavy Xiranite
minimum-rate workload, loose `50 x 50` request ceiling, propagated Rung 1B semantics, and
deterministic Pumpkin search. The baseline and optimized measurements necessarily use separate
builds of their respective source versions. Each five-second row contains four fresh processes.

| Phase | Facilities | Endpoints | Version | Outcome | Search time | Decisions | Conflicts | Solver propagations |
|---:|---:|---:|---|---|---:|---:|---:|---:|
| 23 | 24 | 71 | Allocated direction list | feasible 4/4 | 960-990 ms | 67,165 | 3,028 | 7,849,944 |
| 23 | 24 | 71 | Bit mask + one bound snapshot | feasible 4/4 | 867-891 ms | 67,165 | 3,028 | 7,849,944 |
| 24 | 25 | 75 | Allocated direction list | feasible 4/4 | 103-105 ms | 7,854 | 150 | 1,083,587 |
| 24 | 25 | 75 | Bit mask + one bound snapshot | feasible 4/4 | 90-92 ms | 7,854 | 150 | 1,083,587 |
| 30 | 33 | 105 | Allocated direction list | unknown 4/4 | 5,000 ms | about 212k-218k | about 12.1k-12.4k | about 44.3M-45.5M |
| 30 | 33 | 105 | Bit mask + one bound snapshot | unknown 4/4 | 5,000 ms | 239k-244k | 13.7k-13.9k | 50.4M-51.4M |

The identical Phase 23 and Phase 24 search statistics show that the optimization follows the same
deterministic search path. Only the cost per propagation changes. Phase 23 median wall time falls
from 979.0 to 885.0 milliseconds, a 9.60% reduction and 1.106x throughput. Phase 24 median wall time
falls from 104.0 to 90.5 milliseconds, a 12.98% reduction and 1.149x throughput. At Phase 30 the
optimized code advances approximately 12% farther through the same path within the same
five-second budget.

## Phase 30 First-Witness Confirmation

| Version | Outcome | First witness | Decisions | Conflicts | Solver propagations | Clearance executions |
|---|---|---:|---:|---:|---:|---:|
| Allocated direction list | feasible | 23.711 s | 926,491 | 45,277 | 198,537,975 | 150,557,306 |
| Bit mask + one bound snapshot | feasible | 21.417 s | 926,491 | 45,277 | 198,537,975 | 150,557,306 |

The exact same witness point is reached 2.294 seconds earlier, a 9.67% wall-clock reduction and
1.107x throughput. The solver executes approximately 43.3 thousand branch decisions per second
instead of 39.1 thousand.
This is one paired long confirmation in each version, not a latency distribution. The identical
search trajectory makes the local CPU-overhead attribution strong, but the timing is not presented
as a percentile or repeated-run estimate.

## Interpretation

The experiment confirms that allocation and repeated domain access were measurable local costs.
It also shows that they were not the semantic cliff: Phase 30 still misses the five-second target
and needs more than 21 seconds for its first witness. The remaining cost is dominated by tens of
millions of solver propagations and approximately 150 million endpoint-clearance executions before
the witness.

The Phase 23/24 timing inversion also remains. Therefore the inversion is not caused by the removed
allocation. It continues to reflect deterministic search-order sensitivity in the exact constraint
graph.

## Verification

- `cargo fmt --all -- --check` passes.
- `cargo test -p aic-data endpoint_clearance_propagator --lib` passes all seven focused tests,
  including the exhaustive complete-assignment oracle, differential comparison against the
  reified baseline, all unique-direction deductions, and conflict learning/backtracking.
- `cargo test -p aic-data --features pumpkin-debug-checks endpoint_clearance_propagator --lib`
  passes the same seven tests with Pumpkin runtime checks enabled.
- `cargo check --workspace` passes.
- `cargo test --workspace` passes all workspace and documentation tests, including 345 `aic-data`
  tests.

## Independent Review Resolution

Three independent reviews covered proof soundness and bugs, artifact/report consistency, and exact
optimization strategy.

- The proof review returned **PASS**. It confirmed that a snapshot made stale only by tightening
  can delay pruning but cannot create an invalid deduction, that self-notification restores the
  fixpoint, and that the stack-local implementation is backtracking-safe.
- The artifact and formulation review initially blocked the report on ambiguous build wording, a
  shifted Phase 30 table row, missing median arithmetic, and vague verification accounting. Those
  report defects were corrected; it found no code, semantic, or artifact defect.
- The optimization review returned **PASS**. It confirmed the identical search trajectory and
  recommended a scheduling-priority ablation before the larger event-selective scheduling change.

No review introduced a game-rule change, a heuristic restriction, or an unverified pruning premise.

## Recommended Next Experiment

Keep this optimization. Next, compare `High` and `Medium` scheduling priority without changing any
constraint semantics. The current propagator performs very few useful updates relative to its
wakeups, so running it less eagerly may reduce redundant executions. Treat the priority change as
an isolated exact scheduling ablation and retain it only if repeated Phase 23, Phase 24, and Phase
30 measurements improve.

If priority does not help, the next target is backtracking-safe event-selective dirty scheduling.
That is a larger propagator change and should remain a separate slice.

## Artifacts

- `heavy-xiranite-bottom-up-clearance-hot-path/phase23-run1/` through `phase23-run4/`
- `heavy-xiranite-bottom-up-clearance-hot-path/phase24-run1/` through `phase24-run4/`
- `heavy-xiranite-bottom-up-clearance-hot-path/phase30-run1/` through `phase30-run4/`
- `heavy-xiranite-bottom-up-clearance-hot-path/phase30-30s/`
