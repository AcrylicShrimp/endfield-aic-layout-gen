# Constructive Pipe-Chain Growth

## Purpose

This slice verifies cumulative constructive growth across more than one physical connection. It selects the longest simple facility-to-facility pipe chain in the Heavy Xiranite production graph and grows from its downstream facility toward its upstream suppliers.

Each transaction:

1. keeps every previously accepted facility and pipe fixed;
2. enumerates placements and compatible directional ports for one new facility;
3. routes the new pipe with A* against existing physical occupancy;
4. rejects candidates that eliminate every viable port option for an uncovered chain requirement;
5. compares valid candidates by used bounding-box area, future-port loss, transport tiles, and turns;
6. validates and records the cumulative geometry before advancing.

## Heavy Xiranite Result

- Status: constructed
- Growth pages: 2
- Facilities: 3
- Pipe requirements: 2
- Final used bounds: `17x6`
- Final pipe tiles: 18
- Final turns: 1
- Future port options blocked by accepted candidates: 0
- Release runtime: 1.46 seconds in the recorded warm run

| Growth | Facilities | Bounds | Area | New pipe tiles | Cumulative pipe tiles | Turns |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 2 | `11x5` | 55 | 2 | 2 | 0 |
| 2 | 3 | `17x6` | 102 | 16 | 18 | 1 |

The first connection is direct and compact. The second accepted placement keeps the remaining port feasible and packs all three 5x5 facilities into a nearly full row, but the selected input direction requires the new pipe to travel around the intervening facility. This is valid baseline construction, not a globally optimal complete-chain layout. Transport geometry is cheap, so later relocation or rerouting may replace it only when factory area does not grow.

## Search Evidence

- Placement candidates considered: 10,580
- Overlapping placements rejected: 1,836
- Port pairs considered: 20,640
- Blocked port pairs rejected: 472
- Future-port dead ends rejected: 156
- A* searches: 20,168
- A* failures: 8
- Valid candidates scored: 20,004

An initial run incorrectly measured used space from the local A* canvas origin. That translation-dependent score selected a `17x16` layout with 49 pipe tiles. The score now uses the minimum and maximum coordinates of actual facility and pipe geometry. A translation-invariance regression test covers the correction.

## Visualization

`heavy-xiranite.html` reuses the existing interactive wireframe renderer through neutral geometry-page inputs. It contains `Growth 1/2` and `Growth 2/2`, localized facility labels, newly introduced facility highlighting, pipe arrows, click inspection, and the previous/next controls. No exact-solver report or fabricated solver metric is involved.
