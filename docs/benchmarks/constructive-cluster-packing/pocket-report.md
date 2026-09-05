# Output space screening and local translation refinement

Date: 2026-09-06. Status: complete factory improved; final compactness and full goal remain open.

## Complete results

| Stage | Relative cluster offset | Bounds | Area | Transport tiles | Turns |
| --- | --- | --- | ---: | ---: | ---: |
| Prior complete incumbent | (31, -40) | 125 x 86 | 10,750 | 2,292 | 245 |
| Free-region screen | (27, -45) | 120 x 88 | 10,560 | 2,276 | 246 |
| First radius-two refinement | (25, -45) | 118 x 88 | 10,384 | 2,274 | 245 |
| Second radius-two refinement | (24, -45) | 117 x 88 | 10,296 | 2,273 | 245 |

The final complete original heavy-xiranite witness retains all 59 facilities, 131 exact recipe
terminal margins and seven real external obligations. Area decreases 4.2% relative to the incumbent,
and 20.2% relative to the first 12,900-area complete closure. Component count remains fifteen and is
not an objective. The complete incumbent is independently revalidated and retained throughout.

## Necessary space condition

Previous visual tracing found an eight-cell belt pocket outside F14, too small for F09's 5x5 body.
The new screen derives each pending consumer footprint from its runtime recipe/facility catalog,
not display labels or a recipe-specific rule. For an unused source connection, flood four-neighbor
free cells while treating both fixed facility bodies and that transport layer as obstacles.

Before a legal new consumer is placed, its rectangular body is connected free geometry adjacent to
its input connection. The route connects that connection to the source. Therefore a consumer of
area A needs at least A+1 cells in the source's connected free region. A sealed component smaller
than this cannot support this fixed-interior, no-new-crossing closure. The original pocket reports
two source ports reaching eight cells and one blocked port; the required threshold is 26.

This is only a necessary condition. Opposite-layer transport is relaxed, so body placement space is
overestimated; shape, orientation, approach direction and interaction among multiple consumers are
not proven. Existing consumers do not require new body space. Reaching the measured one-cell outer
halo establishes unbounded free space, not a realizable corridor. An external output needs escape.
The flood stops at a sufficient size threshold, or a 4,096-cell diagnostic budget; capped/unknown
regions remain eligible. A resource bound never becomes a rejection premise or game limit.

Each pending edge requires ceil(rate / catalog transport capacity) separate unused source ports,
matching the current closure contract. Compatibility is a nested size threshold. Sorting demands
from largest to smallest and counting eligible distinct ports is sufficient for this necessary
matching condition. Visited-region results are cached per transport, never across belt/pipe layers.
Neither this screen nor a failed closure proves global factory infeasibility.

The broad contact enumeration still finds 3,360 immediate-port-accessible translations. Apply the
space check lazily while building the diverse shortlist: thirteen checks reject one trapped candidate
and retain twelve. The changed shortlist completes three of 24 corridor-family attempts (two distinct
packings), compared with one of 24 previously. This is a measured local portfolio outcome, not a
general success-rate guarantee. Complete area, not partial area, determines the selected result.

## Local refinement and safety

`--refine-from` identifies an earlier report selecting a complete nested packing. Independently
validate that full reference against current catalogs and the original graph. Its two retained
cluster witness hashes must match the newly constructed clusters exactly. Then enumerate a radius-
one or radius-two integer offset neighborhood, using one-cell separation instead of the broad
portfolio's four-cell diversity rule. No successful offset is hardcoded in application logic.

First refinement enumerates all 25 offsets around (27, -45); 21 pass geometry/access screening and
receive both corridor families. Thirteen of 42 attempts complete. The best lies on the left edge,
so a second neighborhood around (25, -45) is evaluated. Of 25 offsets, twenty are collision-free,
nineteen pass immediate access, and all nineteen pass the size screen. Ten of 38 closure attempts
complete. The winner (24, -45) lies inside this second neighborhood. Its immediate eight neighbors
are covered by the experiment, without finding a better complete result under these closure families.
This is not local optimality: the completion oracle remains bounded heuristic routing/placement.

Each purported complete child is independently rechecked before selection. Refinement cannot
silently replace current clusters with stale ones, accept a partial witness, or discard the incumbent.
The `ports` screen remains an explicit research control; `space` is the nesting default. Shelf
packing and production behavior are unchanged. This is JS research, not premature Rust migration.

## Visual inspection and next boundary

Personally inspected full rendered witnesses after each improvement and the failing one-column
neighbor, including an overlay of the previous valid F09 slot. At offset (24, -45), F09 occupies
(40,66), size 5x5, and F39 starts at x=45. Moving the large cluster one more column left puts F39 at
x=44, overlapping that old F09 slot by one column. The body at the ghost slot is not an accepted
placement: the constructor rejects it and its remaining tried positions exhaust the F14 -> F09
corridors. The screen correctly allows this candidate because the free component is large enough;
available area alone does not establish a suitable rectangular slot and approach.

The current closure placement generator also starts at x=40 (one past the rightmost supplier
connection) and only tries nonnegative x offsets. Moving F09 slightly left at a different row is
therefore untested, not ruled out by game semantics. Next compare a bounded two-sided local
placement neighborhood and explicit escape corridors; retain the same fixed facility orientation,
collision/capacity gates and complete-factory score. A left shift alone is not yet a verified fix:
the nearby seed-return belt also constrains its input approach. Subsequently reduce the wide
structured interfaces inside the larger cluster. Consider caching identical closure attempts across
refinements with a complete algorithm/input fingerprint; repeated neighborhoods currently repeat work.

## Cost, checks and artifacts

| Experiment | Full closure attempts | Complete attempts | Wall time | Packing construction |
| --- | ---: | ---: | ---: | ---: |
| Broad space-screened portfolio | 24 | 3 | 36.51 s | 139.22 ms |
| First refinement | 42 | 13 | 55.57 s | See matrix |
| Second refinement | 38 | 10 | 46.44 s | 128.41 ms |

Broad enumeration/screening took 75.98 ms; the second refinement took 19.83 ms. These are full
optimization portfolios, including independent checks and JSON/HTML output, not a single release
factory generation benchmark. Workspace verification ran concurrently for parts of these experiments;
no controlled wall-time speedup is claimed. Two closure workers run in parallel. Logs remain in
dedicated target files and stdout stays a short completion summary.

- 58 related Node tests pass, including sealed-pocket size boundaries, escape/unknown behavior,
  per-layer cache separation, constructive rectangle controls, 200 exhaustive matching comparisons,
  source immutability and exact local search domain/unique offset tests.
- `cargo check --workspace` passes. Workspace tests: 524 pass, two existing ignores.
- Eight independent final-factory audit expectations pass, including corrupted geometry, missing
  exports, undeclared external substitutions and invalid utilization/terminal reports.
- Browser checks pass for full and diagnostic views, zoom/pan/Fit and six theme/viewport combinations.
- The final source hashes match the current implementation. Regenerating the initial twelve
  space-screened packed witnesses with the final code reproduces every original witness hash.
- Reviewed against AGENTS and the accepted constructive design: production lifecycle unchanged,
  full incumbent retained, external runtime data, area-first ranking and independent acceptance.

Evidence: `pocket-matrix.json`, `refine-matrix.json`, `refine-second-matrix.json`, `pocket-controls.json`.
Final: `http://100.114.24.86:8765/bound-interconnect-closure-pocket-final.html`.
Neighbor: `http://100.114.24.86:8765/bound-interconnect-closure-pocket-neighbor.html`.
Ghost-slot close-up: `http://100.114.24.86:8765/bound-interconnect-closure-pocket-neighbor-fit.png`.

Use the nesting reproduction command from `nesting-report.md` with `--nesting-screen space` and
the previous `nesting-matrix.json` as incumbent. For refinement, use a fresh output directory and
add `--refine-from <previous/report.json> --refine-radius 2`, also using that report as incumbent.
The default refinement budget is 32, enough for the entire 25-offset neighborhood. Referenced source
block and incumbent detail artifacts must exist. Broader recipe coverage, cleanup and the actual
recipe/rate-to-layout web workflow remain unfinished; the full goal is still active.
