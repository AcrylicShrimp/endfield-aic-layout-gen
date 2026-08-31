# Heavy Xiranite Parallel Exact Dimension Sweep Report

## Outcome

The exact dimension portfolio is implemented and successfully applied to Heavy Xiranite phase-zero
network pair `0,1`.

Four independent Pumpkin workers obtained a validated area upper bound of 35, immediately shared it
with the other workers, skipped 59 of 63 exact dimension candidates before they started, and proved
the primary used-area optimum. In the warmed comparison, solver sweep wall time fell from 451 ms
with one worker to 187 ms with four workers, a 2.41x reduction.

The result preserves the full joint placement, rotation, port, external terminal, belt routing,
flow, topology, capacity, and collision problem inside every executed dimension case. No layout or
routing heuristic was added.

## Orchestration

The scheduler uses a crossbeam multi-consumer work queue and one completion channel. Candidate
dimensions are ordered by proven lower-bound area. Every worker owns a completely independent
Pumpkin model.

When a worker validates a feasible witness, it performs this sequence:

1. atomically lower the shared best feasible area;
2. publish its completion event;
3. let every worker read the new bound before accepting its next case.

An unstarted candidate with area greater than the incumbent is skipped. Candidates below or equal
to the incumbent remain eligible. A case already running is allowed to finish, so scheduling races
can execute one dominated case without affecting correctness.

Only a complete witness with independent validation status `passed` updates the upper bound.
Timeouts, unknown results, and invalid witnesses do not prune anything.

## Correctness

The sweep reports the primary area optimum as proven only when every exact candidate below the
final upper bound is proven infeasible.

For this problem:

- `5x6`, area 30: proven infeasible;
- `6x5`, area 30: proven infeasible;
- `5x7`, area 35: validated feasible;
- `7x5`, area 35: validated feasible;
- unresolved smaller candidates: zero.

The resulting primary area optimum is 35. Secondary objectives remain explicitly unproven because
the cases use first-solution search. The observed `5x7` witness has four transport tiles and zero
turns; the observed `7x5` witness has seven transport tiles and zero turns. This comparison selects
the better observed incumbent for visualization but is not a proof that four tiles are optimal.

## Immediate Upper-Bound Propagation

The second four-worker repetition illustrates the intended event flow cleanly:

- workers simultaneously started the first four area-ordered candidates;
- worker 2 validated `5x7` first;
- that result was completion event zero;
- the atomic upper bound changed from infinity to 35 before the coordinator processed the event;
- 59 larger queued candidates were then skipped;
- all smaller cases still completed and closed the primary proof gap.

In the first repetition, one worker finished `6x5` just before the `5x7` result and started `6x6`.
That already-running area-36 case completed as proven infeasible. This is the expected non-cancelling
race described by the contract.

## Performance

| Run | Workers | Sweep wall | Process wall | Executed / skipped | Peak RSS | Area result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Free dimensions | 1 | 5,001 ms search | about 5 s | 1 model / none | 85.14 MiB | no incumbent |
| Sweep repetition 1 | 1 | 468 ms | 0.47 s | 4 / 59 | 88.92 MiB | 35 proven |
| Sweep repetition 2 | 1 | 451 ms | 0.46 s | 4 / 59 | 88.86 MiB | 35 proven |
| Sweep repetition 1 | 4 | 289 ms | 0.70 s | 5 / 58 | 299.77 MiB | 35 proven |
| Sweep repetition 2 | 4 | 187 ms | 0.20 s | 4 / 59 | 298.22 MiB | 35 proven |

The first four-worker process time includes cold dependency/code/data pages and a fifth executed
case, so it is not a stable latency comparison by itself. The repeated sweep-internal measurements
show the relevant solver effect: 451 ms to 187 ms.

Parallelism is not free. Four independent dense Pumpkin models use about 3.36 times the peak RSS of
one worker in the repeated run. Worker count therefore belongs in the orchestration resource policy;
it should not blindly equal the number of dimension candidates.

The CLI serializes a large summary containing every executed model report and also writes per-case
JSON and HTML. Artifact generation is outside `outer_wall_ms` and can materially affect process wall
time in very small solver cases.

## What Became Simpler

The free-dimension model asked one Pumpkin instance to decide its bounding box while the same
dimensions constrained boundary terminals and depended on routed geometry. The portfolio removes
that circular top-level choice from each solver without simplifying placement or routing semantics.

The outer harness now has three simple proof states:

- witness but unresolved smaller cases: upper bound only;
- witness and every smaller case proven infeasible: primary optimum proven;
- no witness and unknown cases: no bound and no infeasibility claim.

No special interpretation of solver timeout is needed.

## Implementation

- `crossbeam-channel` 0.5 is a workspace dependency;
- `dimension_sweep.rs` owns the scheduler and report contract;
- workers update an `AtomicI64` incumbent before sending completion events;
- candidate queue order is deterministic but does not restrict the solution set;
- worker panics become a structured research failure;
- every executed case keeps the two-equality fixed-dimension model identity;
- the CLI automatically writes summary and per-case JSON/HTML artifacts.

Production solving is unchanged. This remains a measured research orchestration candidate.

## Verification

- `cargo fmt --all`;
- `cargo check --workspace`;
- `cargo test --workspace`: 17 CLI tests and 170 data tests passed;
- `cargo build --release --workspace`;
- controlled two-worker scheduler test;
- no skipped candidate at or below the final upper bound;
- unknown smaller case prevents an optimality claim;
- validated witness atomically improves the upper bound;
- two one-worker and two four-worker optimized executions;
- automatic JSON and self-contained HTML artifacts;
- clean distinction between primary-area proof and unproven secondary objectives.

## Artifacts

- Contract: `docs/designs/parallel-exact-dimension-sweep.md`
- Four-worker summary and selected incumbent:
  `docs/benchmarks/heavy-xiranite-parallel-dimension-sweep/summary.json` and `summary.html`
- Per-case JSON and HTML in the same directory;
- Machine-readable comparison:
  `docs/benchmarks/heavy-xiranite-parallel-dimension-sweep/comparison.json`
- Previous fixed-case diagnosis:
  `docs/benchmarks/heavy-xiranite-minimum-rate.twenty-second-exact-dimension-partition-report.md`

## Recommended Next Experiment

Apply the same parallel exact dimension sweep to all three phase-zero networks (`0,1,2`). The old
free-dimension full case returned `unknown`. This test will show whether exact dimension
orchestration moves the cliff beyond the complete first SCC slice or whether the next blocker lies
inside three-network shared routing and flow.

Do not cut this orchestration into production or start cumulative phase growth until the user
reviews the full phase-zero result boundary.
