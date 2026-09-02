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
- Release runtime: 62.13 seconds

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
- Port pairs considered: 459,776
- Blocked port pairs rejected: 36,482
- Future-port dead ends rejected: 6,077
- A* searches: 423,294
- A* failures: 8
- Valid candidates scored: 416,977

The five-stage result is valid, but the runtime is already too high for further naive expansion. Candidate enumeration is not yet the dominant repeated operation: nearly every viable port pair launches an independent A* search. This measured cliff is the next implementation target.

## Visualization

`heavy-xiranite.html` contains all five cumulative phases. Growth 3 is the first belt facility, and Growth 5 is the six-facility mixed-layer result. The renderer reports belt and pipe tile counts separately and preserves legal cross-layer overlap.
