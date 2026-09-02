# Constructive Frontier Growth: Belt Depth 2

## Purpose

This experiment grows the constructive Heavy Xiranite layout through two cumulative belt frontier rings after the initial pipe chain. The frontier depth is now an explicit CLI input and is recorded in schema version 3 reports. Depth `1` preserves the previous six-facility experiment; depth `2` selects eleven internal requirements and attempts to introduce twelve facilities.

The planner still commits the best candidate after each successful phase. It does not yet backtrack to an earlier phase when a later supplier cannot be added.

## Result

- Status: exhausted with valid partial history
- Requested belt frontier depth: 2
- Selected requirements: 11
- Completed requirements: 9
- Completed facilities: 10 of 12
- Final valid bounds: `39x8` (area 312)
- Pipe tiles: 3
- Belt tiles: 26
- Total layer-aware transport tiles: 29
- Release wall time: 113.96 seconds
- Peak worker count: 16
- Peak resident memory observed during the run: approximately 142 MB

| Growth | Item | Facilities | Bounds | Area | New tiles | Total tiles | Turns |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | liquid Xiranite polymer | 2 | `11x5` | 55 | 2 | 2 | 0 |
| 2 | liquid Xiranite | 3 | `17x5` | 85 | 1 | 3 | 0 |
| 3 | Xiranite powder | 4 | `22x6` | 132 | 2 | 5 | 0 |
| 4 | Xiranite powder | 5 | `27x6` | 162 | 2 | 7 | 0 |
| 5 | Xiranite powder | 6 | `32x7` | 224 | 18 | 25 | 2 |
| 6 | enriched carbon | 7 | `35x7` | 245 | 1 | 26 | 2 |
| 7 | enriched carbon | 8 | `36x7` | 252 | 1 | 27 | 2 |
| 8 | enriched carbon | 9 | `39x7` | 273 | 1 | 28 | 2 |
| 9 | enriched carbon | 10 | `39x8` | 312 | 1 | 29 | 2 |

## Failure Breakdown

The tenth attempted phase tried to connect another enriched-carbon furnace to the already placed Xiranite-powder oven. It considered all 23,104 local supplier placements in the current canvas.

The failed phase accounts for the following aggregate-minus-completed-phase work:

- 23,104 placement candidates;
- 1,236 overlap rejections;
- 262,416 compatible port-pair attempts before later checks;
- 131,864 blocked port-pair rejections;
- 130,552 A* searches;
- 130,538 A* failures.

Only 14 A* searches found a physical path. None survived the exact future-port viability check, so the phase had no acceptable candidate. The CLI returned `exhausted`, retained all nine valid prior phases, and wrote the paginated failure visualization automatically.

This is not proof that the complete production graph is impossible. It proves only that the current committed prefix cannot be extended by the planner's bounded local candidate set. Earlier phases chose one locally best layout and made it immutable. The measured next missing capability is therefore bounded local backtracking across earlier constructive choices, not a larger single-phase candidate scan.

## Search Evidence

- Placement candidates considered: 290,752
- Overlapping placements rejected: 10,280
- Port pairs considered: 274,624
- Blocked port pairs rejected: 135,714
- Future-port dead ends rejected: 309
- A* searches: 133,890
- A* failures: 131,006
- Valid candidates scored: 2,503
- Placement area-bound prunes: 257,646
- Endpoint area-bound prunes: 5,020
- Route-cache hits: 0

The work increase is concentrated in the failed phase, not distributed evenly across the nine successful phases. More parallel A* evaluation helps throughput but cannot repair a bad committed prefix.

## Visualization

`heavy-xiranite.html` contains every valid cumulative phase and opens on Growth 9/9 with a `FAILED - PARTIAL HISTORY` banner. The final page shows the ten-facility layout immediately before the failed enriched-carbon connection.

## Next Slice

Add bounded local backtracking to the constructive harness. When a frontier cannot be extended, revisit a small number of immediately preceding accepted choices, try their next-ranked candidates, replay the affected routes, and preserve the first complete valid result. The backtracking budget and explored alternatives must be reported explicitly. Global exhaustive search and global optimality proof remain out of scope.
