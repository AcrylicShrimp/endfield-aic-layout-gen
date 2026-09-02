# Constructive Frontier Growth

## Purpose

This slice continues the completed two-edge pipe chain into the belt layer. Heavy Xiranite has no further internal pipe frontier, so the planner selects every previously unplaced belt supplier connected directly to one of the three pipe-region facilities.

Every phase still performs one complete transaction:

1. place and rotate one new supplier facility;
2. select compatible directional ports for the edge's transport layer;
3. route that layer with A* while treating facilities and same-layer transport as obstacles;
4. reject candidates that make a remaining selected requirement unusable;
5. compare valid candidates by used area, future-port loss, transport tiles, and turns;
6. validate and record the cumulative geometry.

## Heavy Xiranite Result

- Status: constructed
- Growth pages: 5
- Facilities: 6
- Routed requirements: 5 (`2` pipe, `3` belt)
- Final used bounds: `32x7`
- Pipe tiles: 3
- Belt tiles: 13
- Total layer-aware transport tiles: 16
- Final turns: 1
- Release runtime: 0.62 seconds

| Growth | Transport | Facilities | Bounds | Area | New tiles | Total tiles | Turns |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | pipe | 2 | `11x5` | 55 | 2 | 2 | 0 |
| 2 | pipe | 3 | `17x5` | 85 | 1 | 3 | 0 |
| 3 | belt | 4 | `22x6` | 132 | 2 | 5 | 0 |
| 4 | belt | 5 | `27x6` | 162 | 2 | 7 | 0 |
| 5 | belt | 6 | `32x7` | 224 | 9 | 16 | 1 |

The final layout places all six 5x5 facilities in one compact horizontal band. The last belt uses the one-cell strip below the facilities rather than increasing the height further. Transport length remains subordinate to used area.

## Search Evidence

- Placement candidates considered: 61,952
- Overlapping placements rejected: 4,692
- Port pairs considered after placement pruning: 4,134
- Blocked port pairs rejected: 672
- Future-port dead ends rejected: 45
- A* searches: 874
- A* failures: 0
- Valid candidates scored: 829
- Peak parallel workers: 16
- Placement area-bound prunes: 56,773
- Endpoint area-bound prunes: 2,588
- Exact route-cache hits: 0

| Metric | Sequential baseline | Parallel area-bound run | Change |
| --- | ---: | ---: | ---: |
| Runtime | 62.13 s | 0.62 s | 100.2x faster |
| A* searches | 423,294 | 874 | 99.79% fewer |
| Final bounds | `32x7` | `32x7` | unchanged |
| Pipe / belt tiles | 3 / 13 | 3 / 13 | unchanged |
| Final score | `(224, 0, 16, 1)` | `(224, 0, 16, 1)` | unchanged |

Workers share only the best used-area incumbent. A placement is pruned when its facility-only area lower bound is strictly larger, and a port pair is pruned when the area lower bound including both connection cells is strictly larger. Equal-area candidates remain eligible, so later score tiers are not discarded.

Two independent runs produced byte-identical phase geometry, selected ports, routes, and scores. Diagnostic counters vary slightly with worker scheduling because an earlier incumbent can prevent another worker from beginning work; this does not affect the selected result.

The exact endpoint route cache recorded no hits. Facility placement changes the blocked map, and every surviving physical endpoint pair was unique within that map. A global path cache would therefore add memory and hashing without helping this workload. Reusable one-to-many route-search state remains a separate future experiment if routing again dominates after further growth.

## Visualization

`heavy-xiranite.html` contains all five cumulative phases. Growth 3 is the first belt facility, and Growth 5 is the six-facility mixed-layer result. The renderer reports belt and pipe tile counts separately and preserves legal cross-layer overlap.
