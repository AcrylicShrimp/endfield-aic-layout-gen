# Heavy Xiranite Cumulative Exact Dimension Growth Phase-One Report

## Outcome

The cumulative exact dimension portfolio successfully grows the Heavy Xiranite graph through SCC
phase 1. The next first-feasible cliff is later than phase 1.

The release experiment solved phase 0, reused its validated placement as a non-binding hint, rebuilt
the enlarged exact joint model for phase 1, and proved the minimum used area of both phases:

| Phase | Facilities | Networks | Route requirements | Proven minimum area | Sweep wall |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 3 | 4 | 42 | 765 ms |
| 1 | 2 | 6 | 7 | 77 | 3,649 ms |

The complete process took 4.44 seconds and reached the requested phase. No case returned unknown
and no witness failed independent validation.

## Growth Semantics

Phase 1 is a fresh exact model of the cumulative two-facility production graph. The solver remains
responsible for:

- both facility placements and rotations;
- every directional port choice;
- every external boundary terminal;
- all shared belt and pipe cells and directions;
- item assignment, flow, topology, capacity, bridges, and collisions.

The phase-zero incumbent contributes 256 warm-start variable assignments for one retained facility
placement. It contributes no terminal, route-network, logistics-component, or coordinate
constraints. A Phase 1 facility may move away from its Phase 0 position, and every legal Phase 1
solution remains feasible in the model.

## Phase-One Proof

The sound lower-bound report contains:

- minimum width: 5;
- minimum height: 5;
- total facility area: 50;
- at least one mandatory transport cell;
- minimum area: 51.

The bound and the `12x12` caller ceiling produce 47 legal dimension cases. The four workers
executed 23 cases and skipped 24 larger cases after receiving the validated area-77 upper bound.

Every dimension case below area 77 was proven infeasible. The two area-77 cases were both
validated feasible:

| Dimensions | First witness | Observed objective after area |
| --- | ---: | --- |
| `11x7` | 293 ms | 47 transport tiles, 4 turns, 0 components |
| `7x11` | 1,569 ms | 37 transport tiles, 16 turns, 3 components |

The portfolio selected `7x11` because transport tile count is the second objective and 37 is less
than 47. This is the better observed witness, not a proof that 37 tiles or 16 turns are optimal.
Only used area is proven optimal because every case stops at its first validated witness.

## Scaling From Phase Zero

| Metric | Phase 0 | Phase 1 | Change |
| --- | ---: | ---: | ---: |
| Facilities | 1 | 2 | 2.00x |
| Commodity networks | 3 | 6 | 2.00x |
| Route requirements | 4 | 7 | 1.75x |
| Variables per case | 19,582 | 21,745 | 1.11x |
| Constraints per case | 57,845 | 71,395 | 1.23x |
| Constraint incidences | 186,381 | 239,999 | 1.29x |
| Executed dimension cases | 11 | 23 | 2.09x |
| Sweep wall | 765 ms | 3,649 ms | 4.77x |

The runtime growth is larger than the raw model-size growth. The main measured additions are:

- twice as many commodity networks;
- three more route requirements;
- twice as many dimension cases executed before the first upper bound;
- several harder narrow-shape infeasibility proofs;
- `7x11` needing 1,569 ms to produce its first witness.

The portfolio still closes every primary proof gap, so none of these is a cliff yet.

## Upper-Bound Sharing

Worker 1 first validated `11x7`, area 77. It atomically published that area before its completion
event. Workers then skipped every unstarted case above area 77 while allowing already-running and
equal-area cases to finish.

The equal-area `7x11` case later returned the better observed transport-tile count and became the
selected visual incumbent. This demonstrates why equal-area cases are not pruned after the first
area witness.

## Resource Cost

The complete two-phase process measured:

- 4.44 seconds process wall time;
- 770.81 MiB maximum resident set size;
- 160,554,230,513 retired instructions;
- 52,014,728,417 elapsed cycles.

Four independent dense models remain memory-expensive. The result validates exact parallel growth,
but it does not remove the need for an explicit worker and memory policy.

## Current Boundary

The phase-zero dimension/boundary-terminal/routing loop remains closed under growth. Phase 1 also
becomes tractable when dimension selection is partitioned outside Pumpkin.

The next first-feasible cliff has not yet been observed. Phase 2 is the next controlled target. An
older formulation needed the complete `12x12` ceiling for that phase, so it is likely to expose
either:

- the next exact-search cliff;
- a real `12x12` infeasibility under the current external-routing semantics; or
- a feasible area-144 boundary case whose smaller-dimension proofs dominate runtime.

Run the same cumulative command with target phase 2. Do not change the request ceiling, routing
semantics, objective, or worker policy before that comparison. Stop after the Phase 2 result for
user review.

## Verification

- `cargo fmt --all`;
- `cargo check --workspace`;
- `cargo test --workspace`: 18 CLI tests and 171 data tests passed;
- optimized release workspace build;
- controlled two-phase fixture proved both areas and recorded one non-binding placement hint;
- real Heavy Xiranite release execution through target phase 1;
- all feasible witnesses passed independent validation;
- zero unknown and zero invalid-witness cases;
- automatic top-level paginated HTML plus phase and case JSON/HTML artifacts.

## Artifacts

- Contract: `docs/designs/cumulative-exact-dimension-sweep.md`;
- cumulative JSON and paginated HTML:
  `heavy-xiranite-cumulative-dimension-sweep-phase1/summary.json` and `summary.html`;
- phase-local summaries and all executed cases under `phase0/` and `phase1/`;
- machine-readable comparison:
  `heavy-xiranite-cumulative-dimension-sweep-phase1/comparison.json`;
- previous checkpoint:
  `heavy-xiranite-minimum-rate.twenty-fourth-full-phase0-dimension-sweep-report.md`.
