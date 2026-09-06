# Ordinary terminal bends and connected movement

The complete incumbent improves from 4,575 area / 1,383 transport tiles / 211 turns
to 3,942 area / 1,246 tiles / 162 turns (73x54). Area decreases 13.84%. All 59
facilities, 131 recipe terminal margins and seven external obligations remain validated.

The previous A* neighborhood retained every old first/last segment direction even at
ordinary facility connection cells. Current catalog/validator semantics permit a turn
at those cells: the arm facing into the facility is occupied, but the other arm need
not be opposite it. The new terminal-flexible option releases only singleton terminal
anchors with one incident chain and no logistics component. Shared terminal and
component arms remain constrained. This changes the search neighborhood, not game rules.

## Same-parent comparison and continuation

| Report | Scope | Attempts | Selected candidates | Area / tiles / turns | Elapsed seconds |
| --- | --- | ---: | ---: | --- | ---: |
| terminal-control-matrix.json | Retained arms, fixed-path control | 96 | 0 | 4,575 / 1,383 / 211 | 2.128 |
| terminal-fixed-matrix.json | Flexible terminal arms, three rounds | 288 | 31 | 4,380 / 1,258 / 161 | 9.583 |
| terminal-single-matrix.json | Single moves from flexible result | 878 | See stages | 4,380 / 1,214 / 154 | 42.343 |
| terminal-matrix.json | Group moves from flexible result | 864 | See stages | 3,942 / 1,246 / 162 | 53.807 |

The two continuation families ran concurrently. Timings are diagnostic observations,
not speed comparisons: work counts and execution overlap differ. Every selected
candidate passed independent geometry and original graph coverage. Optional XY
post-compaction can change coordinates even in the fixed-path control.

Personally inspected actual fixed-policy and group-result renders. The right-side
approaches and terminal doglegs improve, and more groups occupy earlier empty areas.
However, many same-direction cyclic plant-production regions retain wasteful returns.
No satisfactory compactness or optimum claim is made.

User visual feedback identifies a missing family: touching alternating-orientation
reciprocal-production strips (A/B/A with east/west/east or north/south/north flow).
Current relocation preserves rotations and internal geometry, so it cannot discover
this layout regardless of iteration count. Runtime graph inspection finds seven
two-producer/one-returner cycles and one three-producer/two-returner cycle. The next
experiment should derive reciprocal motifs and compatible opposite orientations
from runtime recipes/catalogs, preserving exact input/output and pending margins.

Verification: 90 related Node tests pass, including singleton/shared/component/branch
eligibility, actual changed terminal arms, unchanged catalog margins, frozen inputs and
detached results. Eight full-factory positive/negative audit controls pass. cargo check
--workspace passes; cargo test --workspace has 524 passes and two existing ignores.

Rendered artifacts: target/shared-results/bound-interconnect-closure-terminal-fixed.png
and target/shared-results/bound-interconnect-closure-terminal-groups.png. Interactive result:
http://100.114.24.86:8765/bound-interconnect-closure-terminal-groups.html

This remains an isolated research improvement pass conforming to transactional complete
validation and area-first comparison in the constructive planner design. The production
constructor, game catalogs, exact solver and Rust dependencies are unchanged.
