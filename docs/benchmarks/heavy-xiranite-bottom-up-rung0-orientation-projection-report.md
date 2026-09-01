# Heavy Xiranite Rung 0 Orientation Projection Report

## Question

The facility-only baseline became unable to find a first witness at 49 facilities. This experiment
tests whether the cliff comes from necessary placement difficulty or from expanding directional
rotations that Rung 0 cannot observe because ports are absent.

The comparison uses the profile defined in
[`search-space-evaluation.md`](../designs/search-space-evaluation.md). Both formulations preserve the
same facility rectangles, ceiling, and pairwise non-overlap semantics. The changed formulation
projects rotations onto distinct occupied-rectangle classes. Full directional rotation returns when
ports become observable in Rung 1.

## Full-Graph Brute-Force Upper Bound

The 59-facility graph contains:

- 21 square `3 x 3` facilities, each with `48 x 48 = 2,304` legal origins;
- 32 square `5 x 5` facilities, each with `46 x 46 = 2,116` legal origins;
- 6 rectangular `4 x 6` facilities, each with two footprint orientations and
  `47 x 45 = 2,115` legal origins per orientation.

After geometry-equivalent rotations are projected together, independently choosing every facility
before checking non-overlap gives:

```text
|Omega_sem| <= 2304^21 * 2116^32 * 4230^6
             ~= 6.12 x 10^198
L_sem^UB     = 660.355 bits
```

The old formulation distinguished all four directional rotations. Compared with the projection,
each square contributed two redundant bits and each rectangle contributed one:

```text
Delta L_sem^UB = 53 * 2 + 6 * 1 = 112 bits
ratio       = 2^112 ~= 5.19 x 10^33
old upper bound ~= 3.18 x 10^232
```

The looser bound `(50 * 50 * 4)^59 = 10^236` ignores facility dimensions. The measured profile uses
the tighter boundary-aware bound above. Neither bound applies pairwise non-overlap, so neither is a
count of legal layouts.

## Growth Profile

`L_sem^UB` is the projected semantic upper bound. `L_directional^UB` is the same upper bound when all
four rotations remain separate. `L_model` is Pumpkin's declared Cartesian variable-domain volume;
it includes auxiliary reification variables and therefore must not be read as independent layouts.

| Phase | Facilities | L_sem^UB | L_directional^UB | Old L_model | New L_model | Old outcome | New first witness |
|---:|---:|---:|---:|---:|---:|---|---:|
| 3 | 4 | 44.19 | 52.19 | 453.15 | 73.15 | feasible | <1 ms |
| 10 | 11 | 122.13 | 144.13 | 3,710.16 | 355.16 | feasible | <1 ms |
| 20 | 21 | 237.34 | 275.34 | 13,803.04 | 1,446.04 | feasible | 2 ms |
| 30 | 33 | 372.76 | 432.76 | 34,362.49 | 3,351.49 | feasible | 7 ms |
| 34 | 47 | 527.42 | 615.42 | 69,996.52 | 6,071.52 | feasible in 3,713 ms | 12 ms |
| 35 | 48 | 538.59 | 628.59 | 73,021.81 | 6,295.81 | feasible in 4,931 ms | 7 ms |
| 36 | 49 | 549.76 | 641.76 | 76,111.10 | 6,524.10 | unknown at 5 s | 12 ms |
| 37 | 50 | 560.93 | 654.93 | 79,264.39 | 6,756.39 | unknown at 5 s | 12 ms |
| 39 | 56 | 627.21 | 733.21 | 99,528.11 | 8,234.11 | unknown at 5 s | 24 ms |
| 40 | 59 | 660.36 | 772.36 | 110,523.98 | 9,026.98 | unknown at 5 s | 26 ms |

## Full-Graph Model And Search Work

| Metric | Directional-rotation baseline | Geometry projection | Change |
|---|---:|---:|---:|
| Variables | 109,917 | 8,479 | 92.29% fewer |
| Constraints | 138,001 | 10,565 | 92.34% fewer |
| `L_model` | 110,523.98 bits | 9,026.98 bits | 101,497 bits removed |
| Build time | 393 ms | 17-18 ms | about 22x faster |
| First validated witness | none within 5,005 ms | 26 ms, 4/4 runs | more than 192x faster |
| Branch decisions | 25,645 before timeout | 784 | baseline spent 32.7x more without a witness |
| Conflicts | 3,688 before timeout | 43 | baseline spent 85.8x more without a witness |
| Solver propagations | 18,783,847 before timeout | 174,818 | baseline spent 107.4x more without a witness |

The `L_model` quotient is `2^101497`, approximately `10^30553.64`, but that number is deliberately
not called a speedup. Most removed Boolean variables were auxiliary rotation-pair separation
states. The observed search counters and first-witness time are the performance evidence.

## Conclusion

The old Rung 0 encoding's first-witness cliff was caused by expanding contract-invisible
directional rotations into every pairwise non-overlap disjunction. The exact projection removes only
equivalent geometry states, and the projected facility-only feasibility model finds a validated
witness for all 59 facilities robustly in 26 ms. This is not a compact-layout optimum and does
not yet measure the directional rotation and port cost of Rung 1.

This does not prove that rotation will be cheap in Rung 1. Ports make directional rotations
observable. The next experiment must reintroduce full rotation through a strongly channelled
placement-and-port formulation, grow it through all cumulative phases, and find the next cliff.

## Artifacts

- `heavy-xiranite-bottom-up-rung0-full/`: committed directional-rotation baseline at 59 facilities.
- `heavy-xiranite-bottom-up-rung0-growth/`: committed baseline growth sweep.
- `heavy-xiranite-bottom-up-rung0-orientation-projection/`: geometry-projected growth sweep and four
  repeated full-graph runs.
