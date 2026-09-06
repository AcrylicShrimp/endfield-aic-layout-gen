# Connected-group relocation into measured voids

## Outcome

The complete heavy-xiranite witness improves from 83x69 (5,727 area) to 75x61
(4,575 area): 20.12% less bounding-box area. Physical transport tiles increase
from 1,321 to 1,383; turns increase from 195 to 211. This is an improvement under
the required lexicographic objective: whole-factory bounding-box area, physical
transport tiles, then turns. Components remain diagnostic only (15).

All 59 original facilities, 131 recipe terminal margins and seven real external
obligations remain independently validated. No pending recipe terminal is omitted.
This is neither a minimum-area result nor completion of the broader product goal.

## Geometric change

The previous single-facility neighborhood cannot carry shared internal junctions
or zero-length connections. The new movement DTO v2 replaces absolute single-body
requests with a selected instance set and a relative translation. Singles and groups
now share one transaction kernel; there is no legacy request adapter.

Groups are derived from graph-connected pairs, serial triplets and discovered cycles,
with optional direct consumers. Internal chains and exclusively owned junctions move
with their group. Boundary chains and unrelated wires displaced by the move are
reconstructed with A*. All unaffected geometry and exact commodity rates are retained.
Only complete, independently validated, better candidates replace the incumbent.

Candidate positions include body contacts and corners of measured empty rectangles.
Collision checks use actual occupied geometry, not solid module bounding boxes.
Separate void, packing and locality portfolios permit longer boundary connections.
For example, moving F57/F53/F52 into a void increases endpoint-distance estimates
by 25 while reducing complete area to 5,025. Wire distance is not an area proxy or
a prerequisite for acceptance.

Accepted later moves carry two junction components and six or seven internal chains.
The F09/F04/F01 group also moves together. XY difference-constraint compaction is
optional postprocessing, not a substitute for changing relative placement and paths.

## Recorded experiments

These reports form a sequential development history, not repeated trials of one
unchanged source revision. Each report records its own source/input hashes.

| Report | Scope | Attempts | Area | Tiles | Turns | Elapsed seconds |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| group-contact-matrix.json | Contact group moves, one round | 128 | 5,508 | 1,307 | 193 | 13.516 |
| group-void-matrix.json | Void positions, two rounds | 384 | 5,025 | 1,376 | 203 | 35.253 |
| group-joint-matrix.json | Paths plus groups, two rounds | 576 | 4,725 | 1,424 | 209 | 39.464 |
| group-improvement-matrix.json | Paths plus groups, three rounds | 864 | 4,575 | 1,383 | 211 | 55.777 |
| group-matrix.json | Current-source group neighborhood control | 192 | 4,575 | 1,383 | 211 | 14.549 |

The final control tries 96 positions in both near/far routing orders and retains
the incumbent. This only exhausts that explicit shortlist from that parent, not
all group moves, port choices or complete layouts. Raw witnesses remain under the
reported target directories; committed reports preserve structured diagnostics.

## Visual inspection

Personally inspected actual rendered whole-layout images after each improvement.
The largest measured empty rectangle decreases from 28x32 (896 cells) to 21x14
(294 cells). Final occupied XY cells total 2,418 of 4,575; 2,157 cells remain unused.
This occupancy measurement is diagnostic, not the area objective.

The upper-left void, F01's long process connections, nested return structures and
right-side corridors remain visibly inefficient. A validator pass does not make
them desirable. Fixed external endpoints, retained port arms and rigid internal
group geometry constrain this neighborhood. Flexible boundary/junction placement
and port choices are follow-up hypotheses, not demonstrated solutions.

Actual render: `target/shared-results/bound-interconnect-closure-group-final.png`.
Interactive tailnet page:
http://100.114.24.86:8765/bound-interconnect-closure-group-final.html

## Profiling and verification

CPU sampling identifies repeated movement planning and full-state copying as larger
costs than A* in this neighborhood. Failed trials now use private append-only drafts;
successful results are fully detached. No input mutation is permitted.

The same 192 requests before/after deferred copying reproduce identical statuses,
routes, affected chains, diagnostics and raw/final scores. Two single CPU-profiled
runs measure 16.573s before and 14.715s after; sampled structuredClone self time falls
from 2,784.687ms to 890.848ms. These are single observations, not a controlled speedup
claim. Movement planning remains the largest named sampled hotspot (3,124.241ms in
the latter run). Raw profiles stay in target/local-groups*.cpuprofile.

Verification:

- 88 related Node tests pass, including frozen-input and detached-success checks.
- All 1,024 tiny occupancy masks match brute-force largest-empty-rectangle results.
- Group tests cover internal junctions, zero-length shared ownership, DTO rejection
  and current-extent bounds. Existing directed A* exhaustive tests also pass.
- Eight independent complete-factory audit controls pass (group-controls.json).
- cargo check --workspace passes; cargo test --workspace: 524 pass, two existing ignores.
- Current-source hashes match group-matrix.json. Relevant CLI runs emit one short
  completion summary; detailed candidate data is written to JSON files.

This is a research improvement pass over a complete valid factory, not a change to
production construction order. It preserves transactional validation and the area-first
score required by the constructive planner design and repository policy. No Rust
migration, global infeasibility claim or production solver dependency was introduced.
