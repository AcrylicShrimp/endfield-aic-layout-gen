# M4: bounded parallel commodity screening

## Status

Complete. Baseline is `86bd588` (M3). Adopt a default ceiling of four workers, supported by
five rotated unprofiled samples per variant/workload plus separate profiles (60 CLI runs total).
All complete/intermediate witnesses, search choices and ordered diagnostics match exactly.

## Contract and deterministic execution

The [pre-implementation contract](../../todos/2026-09-05.09-bounded-parallel-analysis.md) limits
parallelism to independent commodity screens of one immutable candidate. Growth, validation, port
assignment, physical routing and first-feasible acceptance remain sequential. No speculative
candidate placement or race-selected winner is introduced.

The request and CLI accept `analysis_workers` / `--analysis-workers`, from one through 16. One
executes the serial path. The adopted default is four; actual workers are further capped by
host availability, the number of commodity jobs and aggregate auxiliary workspace capacity.
All inputs are immutable; each worker receives an independent scratch copy. Occupancy is built
once, then copied into bounded worker workspaces before any thread starts.

Sorted (transport, item) jobs are assigned exactly once through a bounded atomic counter. Workers
return job-indexed outcomes; the caller joins every worker, sorts outcomes by original job index,
then sorts/deduplicates blocked requirement IDs. All observed visited/probed work is aggregated on
the caller. Workspace state is reset between jobs and never survives a candidate mutation.

The 64 MiB auxiliary guard covers the base plus all worker workspaces. If parallel copies do not
fit, fewer workers are selected; a one-worker screen uses its original single workspace. Thread
stacks, temporary crossing maps and input/output history are separate from this auxiliary capacity
and are included in process RSS. The worker ceiling also bounds thread-stack and scheduler costs.

Allocation completes before threads start. Thread creation failure or a worker panic returns
`production-boundary-worker-spawn-failed` or `production-boundary-worker-panicked`, never an
inaccessible-boundary or infeasibility result. Cancellation stops new job claims; already-running
jobs finish and every started worker is joined before returning. Partial outcomes are not accepted.
Existing structured screening failures retain prior validated geometry.

## Worker-aware profiling

Each enabled worker captures its own local operation stack. The parent merges bounded aggregates
only after joins. Profile schema 1 gains optional `workers`, with captures, busy captures, completed
jobs, summed worker wall duration, maximum concurrently live workers, bounded per-worker-slot load
and operation aggregates. The scope changes to
`calling-thread-with-separate-worker-aggregates` when worker captures are present; pure serial
runs retain `calling-thread-only` and omit `workers`.

Main `elapsed_ns` and `boundary-dispatch` are critical-path wall timings. Worker `wall_ns_sum` and
worker operation times overlap each other and the parent; they are neither elapsed time nor process
CPU time and must not be subtracted from or added to the parent span stack. Main dispatch-exclusive
time includes waiting, not only computation. Native process user/system time is measured separately.
Busy captures and per-slot job distribution describe scheduling load; concurrently live workers
do not imply that every worker used a CPU continuously.

Parent work counters preserve total commodity screens, visited cells, neighbor probes and crossing
hops. New counters include chosen worker slots, parallel screens and peak chosen worker count.
Workspace byte sum/peak now accounts for simultaneous base-plus-worker copies. Fixed-size operation
and slot maps remain bounded; no per-job execution history is streamed to stdout or accumulated in
the production report. Full report/history still uses file I/O and stdout is one short completion.

## Compatibility and tests

The request remains schema 1 with an optional `analysis_workers` field defaulting to four. Serialized
report requests now include it; strict consumers must allow this additive field. Users requiring
serial execution can specify one. CLI help exposes the ceiling and host/job/memory caps explicitly.
Structured invalid-input diagnostics reject zero and more than 16 workers.

Tests compare complete/intermediate witnesses, statistics and diagnostics for one/two/four/eight
requested workers. Forced multi-commodity screens verify worker profiles and exact aggregate work
against serial execution across repeated calls. Existing reference comparisons now also run four
workers on commodity/layer mutations, obstructed endpoints and translated geometry. Worker-count
tests cover host, job and memory caps. A controlled panic verifies cancellation and that the other
worker is joined before returning; profiling tests verify worker durations never enter the parent
exclusive-time arithmetic. The thread-creation error is handled explicitly but OS exhaustion is
not induced in tests.

## Measurement protocol

The [harness](run.mjs) compares the preserved M3 binary with the candidate at one, two, four and
eight requested workers. Five unprofiled rounds rotate variant order so each variant occupies every
position once. A separate profiled round follows. Both Heavy Xiranite and the existing seed-cycle
workload retain identical catalogs, source plans, seed, search budgets and full JSON/HTML output.
All compiler and test processes finish before timing; all samples are retained.

```sh
node docs/benchmarks/constructive-parallel-analysis/run.mjs /absolute/path/baseline-aic-cli target/release/aic-cli /absolute/path/new-experiment-directory
```

The macOS harness records native CPU/RSS, wall/planner latency, input/binary hashes and profile
data, and checks exact final/intermediate witness and search-choice signatures. Large raw artifacts
remain outside git. `--profile-only` selects a separate diagnostic matrix without manufacturing
unprofiled distributions. No worker count is a game bound or a layout W/H input.

M4 does not change the accepted constructive design or objective ordering. The following M5 unit
closes this runtime phase and specifies a bounded transactional layout-improvement pass; it does
not silently start a new global optimizer or remove independent validation.

## Results and adoption

The [machine-readable evidence](summary.json) records every sample, input/binary hashes and native
CPU/RSS. Times below are seconds; brackets are the five-sample range. RSS is decimal MB.

| Heavy Xiranite | CLI median [range] | Planner median [range] | RSS median [range] | User / system median |
| --- | --- | --- | --- | --- |
| M3 baseline | 5.550 [5.525, 6.005] | 3.867 [3.863, 3.880] | 284.312 [283.771, 285.344] | 5.16 / 0.36 |
| Candidate, 1 | 5.544 [5.498, 5.915] | 3.859 [3.812, 3.877] | 284.836 [284.738, 285.147] | 5.14 / 0.37 |
| Candidate, 2 | 5.079 [5.042, 5.097] | 3.403 [3.384, 3.412] | 285.065 [284.918, 285.655] | 5.26 / 0.39 |
| Candidate, 4 | 4.828 [4.798, 4.838] | 3.137 [3.128, 3.150] | 286.654 [286.212, 286.818] | 5.33 / 0.39 |
| Candidate, 8 | 4.782 [4.762, 4.793] | 3.101 [3.092, 3.107] | 286.867 [286.622, 288.211] | 5.49 / 0.41 |

Four workers reduce planner latency 18.7% and full CLI latency 12.9% relative to the candidate's
serial path. Median RSS rises 1.819 MB versus that serial path, or 2.343 MB (0.82%) versus M3.
Eight workers save a further 45 ms of full CLI / 36 ms of construction, but consume more CPU,
workspace and worker starts. Four is the measured default tradeoff; eight remains an explicit option.
This is a latency improvement, not a reduction in total CPU consumption or a claim of linear scaling.

The small seed-cycle workload automatically stays serial because each screen has one commodity.
CLI medians for baseline/1/2/4/8 are 30.324/30.214/30.221/30.170/30.040 ms; ranges overlap.
All planner medians are 2 ms and median RSS stays 8.405-8.454 MB. No meaningful small-case speedup
or regression is claimed. Forced multi-commodity unit cases separately exercise parallel correctness.

### Separate profile accounting

| Heavy Xiranite profile | 1 worker | 2 workers | 4 workers | 8 workers |
| --- | ---: | ---: | ---: | ---: |
| Boundary screening wall, s | 1.183 | 0.701 | 0.453 | 0.414 |
| Dispatch/join wall, s | n/a | 0.697 | 0.449 | 0.410 |
| Sum of worker wall, s | n/a | 1.276 | 1.339 | 1.514 |
| Captures / busy captures | n/a | 184 / 184 | 368 / 367 | 654 / 573 |
| Peak concurrent workers | 1 | 2 | 4 | 8 |
| Peak auxiliary workspace, bytes | 258,100 | 774,300 | 1,290,500 | 1,831,680 |

At four workers, slots complete 156/173/164/196 jobs, with summed slot wall times
0.358/0.382/0.339/0.261 s. Load is not perfectly balanced; launching more workers does not remove
the longest commodity job. Dispatch includes allocation/copy, launch, useful work and joins; it
is not an isolated scheduler-overhead measurement. The 8-worker captures include 81 idle captures.

All variants perform 92 screens, 689 commodity floods, 7,509,261 visited cells, 30,037,044 neighbor
probes and 629,525 crossing hops. With four workers, main crossing calls are 178 and worker calls
689, preserving the serial total of 867. Worker flood calls are 689. Main critical-path screening
falls to 0.453 s; summed worker crossing/flood times are 0.978/0.301 s and overlap that interval.
Independent validation remains 0.978 s, while report serialization/write is 0.656 s and HTML
rendering 0.977 s. These are the remaining measured costs, not grounds to weaken validation.

## Verification and remaining risk

- `cargo fmt --all`, `cargo check --workspace`, `cargo test --workspace --quiet`: passed;
  492 tests passed and two existing research tests remained ignored.
- `cargo build --release -p aic-cli` completed before timing; harness syntax and `git diff --check`
  passed. All 60 CLI runs completed, retaining 59 facilities, 89 internal/seven external requirements,
  47 accepted states and 178 A* calls for Heavy Xiranite.
- Manual full-workload CLI without a worker flag selected four and matched final witness SHA-256
  `acd24c232943a16a53455e6de1b6aea8d9bd95137ab5302568f5b15e9f2e6009`; stdout was one 218-byte
  completion line. A manual zero-worker request exited one, saved `production-input-invalid` in
  JSON and emitted one 188-byte failure summary with no visualization.
- No new dependency, global exact invocation, geometry/search change, speculative growth or W/H
  input was introduced. The shared serial/parallel commodity kernel replaces duplicated execution
  logic; profiling explicitly separates worker aggregates rather than reusing a serial-only model.
- Measurements cover this host and these workloads, not every CPU topology. Thread-start failures
  are handled but not forced against the OS. A worker panic is detected when its ordered join is
  reached; cancellation is not a preemptive interrupt of another in-flight commodity job.
