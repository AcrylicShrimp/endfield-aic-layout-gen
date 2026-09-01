# Heavy Xiranite bottom-up endpoint-clearance shard A/B

## Result

The exact target-facility shard reformulation is semantically sound and measurably cheaper, but it
does not cross the Phase 31/32 first-witness cliff.

- Phase 29 first witness improved from a 215 ms median to 180 ms (`-16.28%`). All 12 witnesses,
  six from each formulation, passed validation.
- Under a fixed decision budget, shard CPU cost improved by `5.88%` at Phase 29, `9.59%` at Phase
  31, and `9.28%` at Phase 32.
- Peak RSS changed by `+1.07%`, `+0.99%`, and `-1.32%` respectively. There is no material memory
  regression.
- During the five-second runs, the shard formulation processed `9.22%` more decisions at Phase 31
  and `8.98%` more at Phase 32.
- Both formulations still returned `unknown` without a first witness at Phase 31 and Phase 32.

The experimental rung is retained as reproducible evidence, but it does not replace the pairwise
baseline. It missed the predeclared `>=15%` fixed-decision CPU improvement gate and did not produce
a new Phase 31/32 witness.

## Exact reformulation

The baseline registers one custom propagator for every endpoint and non-owner target facility:

```text
endpoint-clearance relations = terminals * (facilities - 1)
```

The shard formulation transposes the same Cartesian relation set into one propagator per target
facility. A target facility shard watches its own position and orientations once and watches every
non-owner endpoint position. It evaluates the unchanged relation kernel and emits the unchanged
relation-specific reasons.

This is a scheduling reformulation only. It does not fix placement, rotation, port choice, or any
endpoint coordinate, and it does not remove a legal solution.

## Semantic gates

The following tests passed for the pairwise and shard formulations:

- exhaustive complete-assignment equivalence on two relations;
- partial-domain root-fixpoint equivalence with two surviving orientations;
- false-orientation event filtering on/off equivalence;
- initial full execution without a watched event;
- a later shared-facility bound update re-dirtying an earlier relation;
- endpoint-only versus full-shard batch scope;
- conflict learning and backtracking with a sibling relation reconsidered after recovery; and
- all six shard tests under Pumpkin `debug-checks`, which exercises scratch propagation.

Three independent reviews returned PASS after the initial missing-first-execution and ambiguous
counter semantics were corrected.

## Performance protocol

All authoritative timing runs used one release binary, High priority, the false-event filter off,
the 50x50 caller ceiling, counters off, serial execution, and counterbalanced pairwise/shard order.
Each phase and formulation was run in six fresh processes.

The fixed-decision controls used limits derived below the slowest five-second pilot throughput:

| Phase | Facilities | Terminals | Decision limit |
| ---: | ---: | ---: | ---: |
| 29 | 30 | 95 | 10,000 |
| 31 | 36 | 115 | 200,000 |
| 32 | 39 | 125 | 180,000 |

Each fixed-decision run also had a ten-second safety guard. Every run stopped due to the decision
limit, not the time guard.

## Five-second time-budget medians

| Phase | Formulation | Outcome | First witness | Decisions | Conflicts | CPU | Peak RSS |
| ---: | --- | --- | ---: | ---: | ---: | ---: | ---: |
| 29 | pairwise | feasible, valid | 215 ms | 13,332 | 344 | 0.235 s | 33.06 MB |
| 29 | shard | feasible, valid | 180 ms | 11,830 | 282 | 0.200 s | 33.31 MB |
| 31 | pairwise | unknown | - | 252,958.5 | 11,057 | 5.025 s | 48.05 MB |
| 31 | shard | unknown | - | 276,282 | 11,718 | 5.025 s | 48.36 MB |
| 32 | pairwise | unknown | - | 220,275.5 | 11,045 | 5.030 s | 53.26 MB |
| 32 | shard | unknown | - | 240,060.5 | 11,789 | 5.020 s | 54.34 MB |

Native `solver_propagations` is intentionally excluded. One shard invocation can evaluate many
semantic relations, so its propagation count has a different unit from the pairwise formulation.

## Fixed-decision medians

| Phase | Formulation | Search | CPU | Conflicts | Peak RSS | Shard CPU change |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 29 | pairwise | 156.0 ms | 0.170 s | 244 | 30.49 MB | - |
| 29 | shard | 146.5 ms | 0.160 s | 247 | 30.82 MB | `-5.88%` |
| 31 | pairwise | 3,779.5 ms | 3.805 s | 8,120 | 44.89 MB | - |
| 31 | shard | 3,418.5 ms | 3.440 s | 8,017 | 45.33 MB | `-9.59%` |
| 32 | pairwise | 3,905.5 ms | 3.935 s | 8,178 | 51.06 MB | - |
| 32 | shard | 3,545.0 ms | 3.570 s | 7,973 | 50.39 MB | `-9.28%` |

The fixed decision count does not imply an identical predicate trace. It measures cost per explored
decision while retaining the solver's native brancher and exact feasible set.

## Telemetry interpretation

Counters-on runs are descriptive only and are not timing evidence.

| Phase | Formulation | Relations | Physical callbacks | Physical executions | Actual relation checks | No-effect checks |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 29 | pairwise | 2,755 | 1,908,932 | 1,543,790 | 1,543,790 | 1,542,540 |
| 29 | shard | 2,755 / 30 shards | 714,039 | 317,220 | 1,484,588 | 1,483,411 |
| 31 | pairwise | 4,025 | 43,373,491 | 34,674,272 | 34,674,272 | 34,642,488 |
| 31 | shard | 4,025 / 36 shards | 18,042,625 | 9,271,188 | 36,622,035 | 36,588,548 |
| 32 | pairwise | 4,750 | 44,399,761 | 35,130,490 | 35,130,490 | 35,099,177 |
| 32 | shard | 4,750 / 39 shards | 16,768,924 | 8,626,927 | 39,379,685 | 39,344,294 |

Within each shard trace, batching coalesces `59.82%`, `60.14%`, and `65.13%` of logical
notifications into fewer physical callbacks at Phases 29, 31, and 32. It also coalesces `78.63%`,
`74.68%`, and `78.09%` of actual relation checks into fewer physical shard executions. These
within-trace ratios avoid comparing absolute counts from different five-second search trajectories.
However, semantic relation checks remain approximately `99.91%` no-effect in both formulations.
The shard run can therefore process more search decisions, but it still repeatedly checks millions
of relations that do not change a domain.

This explains both sides of the result:

- the 6-10% fixed-decision CPU improvement is consistent with net savings from shard scheduling and
  batch coalescing; and
- sharding alone is insufficient to cross the Phase 31/32 cliff, while the remaining no-effect
  relation checks identify relation-level work as the next suspect rather than its proven sole cause.

## Improvements retained from the research chain

The current exact research baseline now includes:

- shared belt and pipe layer state instead of one dense routing grid per logical network;
- placement and port state factored instead of flattening every placement-port product;
- external connectors represented as ordinary exact routing choices rather than hand-shaped paths;
- exact dimension and rotation partitions as orchestration experiments rather than heuristics;
- explicit bottom-up semantic rungs that reveal the first feature cliff before full routing;
- custom propagators with branch/backtrack/conflict/learned-clause/propagation counters;
- a mathematical game-rule reference and mandatory independent propagator reviews;
- endpoint-clearance relation hotset and event-yield telemetry; and
- the target-facility shard experiment reported here.

## Next exact diagnostic

The next experiment should not add another optimization blindly. First split shard work into:

- scheduled versus actually checked relations for full-shard batches;
- scheduled versus actually checked relations for endpoint-only batches;
- relation effects and entailment state by trigger family; and
- facility-X, facility-Y, and orientation causes of full-shard wakeups.

If full-shard wakeups dominate avoidable checks, test an exact axis-sensitive dirty mask. If already
entailed relations dominate repeated checks, test exact entailment dormancy with correct backtrack
reactivation. Neither should be stacked into this A/B result.

## Reproduction

The machine-readable aggregate is
`docs/benchmarks/heavy-xiranite-bottom-up-clearance-shards-ab/summary.json`. Raw self-contained HTML,
JSON, and `/usr/bin/time -lp` outputs are under its `raw/` directory.

The normal time-budget command shape is:

```bash
target/release/aic-bottom-up-ladder \
  --rung facility-ports-sharded \
  --endpoint-clearance-priority high \
  --disable-endpoint-clearance-counters \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --placement-request data/benchmarks/requests/placement.50x50.request.json \
  --target-phase 32 \
  --time-limit-ms 5000 \
  --output-dir /tmp/aic-shard-phase32
```

Add `--decision-limit 180000` and use a ten-second time guard for the Phase 32 fixed-decision
control.
