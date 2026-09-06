# Direct monotone crossings and a smaller complete factory

Status: shared research geometry replaced and independently verified. The new best
complete Heavy Xiranite factory is **104x30 / 3,120 area / 868 transport cells / 106
turns**, all 59 facilities, 131 recipe margins and seven true external obligations.
This improves the prior 3,224-area incumbent by 3.23%. Area remains first: physical
transport increases from 775 to 868. The broader factory/product goal is not complete.

## Geometry cutover

The old compiler decomposed a permutation into adjacent exchanges, reserved 6x6
boxes, and placed dependency stages in successive columns. Independent exchanges
already shared stages. The real defect was not missing parallelism: repeated exchange
paths needlessly left their lane bands and returned, while unchanged lanes traversed
every intermediate stage.

The new compiler plans one monotone channel for the whole permutation. Moving lanes
use vertical intervals; non-overlapping intervals share columns. Precedence avoids
redundant crossings between non-inverted pairs. Each path moves directly toward its
final row, with a one-cell exit jog. Unchanged lanes remain straight. Bridge cells
are instantiated only where the resulting perpendicular paths actually intersect,
using runtime catalog arms, rotations and capacity. Items and lane rates are unchanged.

Every channel path is Manhattan-shortest for its fixed endpoints. Each inverted pair
crosses exactly once; non-inverted pairs do not cross. Stable same-item lanes therefore
remain separate without illegal same-item bridges. The proof sketch and constraints
are in `compact-contract.md`; the independent cell checker does not use this proof or
the construction order to accept geometry. The initial 5x4 two-lane experiment also
passed, but was superseded by the whole-channel construction before this commit.

Schema 2 removes stage/swap scheduling and emits track columns, interval precedence
and actual crossing count. There is no old-generator fallback or adapter. This makes
the geometry simpler: no padded exchange boxes, intermediate lane orders or staged
path stitching. The separate full-factory constructor and Rust production API are
unchanged. Greedy track coloring, three-cell boundary pitch and surrounding approach/
tree geometry remain restrictions; no global minimum-area theorem is claimed.

## Fixed-interface evidence

Replayed the exact lane commodities, rates and boundary order from the prior maximum-
cut first frontier. Both old and new geometry pass current independent cell checking.
These dimensions are **before compaction and facility binding**, not the final factory.

| Interface | Bridges retained | Old dimensions / cells | New dimensions / cells |
| --- | ---: | --- | --- |
| Six lanes | 2 | 12x18 / 90 | 5x16 / 40 |
| Twelve lanes | 12 | 36x36 / 540 | 10x34 / 180 |

The twelve-lane core reduces area by 73.77% and transport cells by 66.67%, without
removing any required crossing. A two-lane exchange is 5x4 / 15 cells instead of
6x6 / 21. A three-lane reversal uses 5x7; its middle lane stays straight through two
bridges rather than moving down and then back up through three exchange stages.

## Complete-factory controls

The source graphs, catalog files, source-bank inventory and candidate limits match
the respective prior controls. New source hashes include the new track planner.

| Constructor | Old complete closure trials | New complete closure trials | Best new raw area | After three improvement rounds |
| --- | ---: | ---: | ---: | ---: |
| Minimum-supported SCC frontier | 1/8 | 4/8 | 6,225 | 4,161 |
| Maximum-supported SCC frontier | 1/8 | 1/8 | 6,708 | 4,680 |
| Previous occupied-shape constructor | 2/24 | 2/24 | 4,680 | 3,286 |

An additional lower-wire minimum-cut seed (raw area 6,936) improves to 3,772 in three
rounds. This is an explicit extra seed, not a replacement for the same-seed comparison.
The shape rows each contain twelve packings and two corridor trials per packing. Both
corridor families complete the same one packing: **one complete packing, two complete
closure trials**. Earlier reports' "1/24" conflated these counts. The compact evidence
now records `complete_packings` and `complete_trials` separately.

The new shape seed improves 4,680 -> 3,286 -> 3,120 over two three-round batches. For
comparison, giving the old 3,224 incumbent three additional rounds reaches 3,162.
Thus the final new result is also 1.33% smaller than the additionally improved old
incumbent. The 3.23% headline includes further improvement work; it must not be
attributed entirely to the crossing change. At the first three-round checkpoint,
the new shape result was still worse than the old 3,224 result.

The new frontier experiment times are 26.188s / 25.352s for eight closures; the shape
experiment takes 39.130s for 24 closure trials. The two shape improvement batches take
38.927s / 38.540s for 1,068 attempts each. These are observed experiment wall times,
some under concurrent load, not single-factory generation costs or CPU speedup claims.
Full timings and candidates remain in the referenced JSON/log directories.

## Visual audit and next geometry gate

Personally inspected native standalone permutation, recipe-bound local interface,
raw full factories, crossing close-up, intermediate improvement and final 3,120-area
renders. The staircase/U-return behavior is removed inside the new permutation core.
Surrounding approaches and later re-routing may still detour; this is not a claim that
every final factory route is globally shortest.

The new complete witness occupies 1,929 XY cells, with 1,191 empty cells (38.17%). The
largest empty rectangle is 12x9 / 108, down from 32x6 / 192 in the prior 3,224 witness.
The final image still shows conspicuous nested yellow return loops around the inherited
F32/F45/F33/F46/F38 region, and long cyan/magenta leads into the now narrow crossing
band. Those are separate source-cycle and interface-approach problems. The result is
better but still visibly wastes space.

Next: rebuild the large source SCC's concrete reciprocal geometry from exact runtime
margins, then attach its dependent cycle/consumers transactionally. Do not just compress
the old nested loops. Preserve already validated unrelated source banks and compare
complete combined area. Continue stop/grow grouping and source/consumer slot studies;
do not treat one fill threshold or this crossing layout as the final packing solution.

## Verification and artifacts

- 134 related Node tests pass. Exhaustive checks cover all 873 distinct-item permutations
  through six lanes, stable binary commodity words, and sixteen deterministic larger
  permutations through 32 lanes. Every tested lane is shortest, all inversions are
  realized exactly once, and catalog/geometry mutations are rejected. Both transport
  catalogs and restricted directional component rotations are covered.
- `cargo check --workspace` passes. `cargo test --workspace`: 524 pass, zero failures,
  two existing ignores. No Rust edits or new dependencies.
- Eight independent original-graph audit controls pass for the final 3,120 witness,
  the additionally improved 3,162 control and the 3,772 frontier alternative. The
  evidence writer checks the original graph hash and exact audited witness identity.
- Actual recipe-bound leaf interfaces pass the independent release validator, including
  shared splitter trees and the new crossing channel. The existing trunk alternative
  remains a valid smaller choice in its local fixture; it was not removed to favor the
  new geometry.
- Visual inspection caught an unrelated stale local `accepted:false` serialization:
  independently accepted local results displayed planner REJECTED. Fixed the local
  runner to publish acceptance after validation, while keeping `factory_validated:false`
  for this local-only fixture. Successful/failure CLI behavior was checked; the corrected
  native image shows PASS without a false rejection label.
- Native browser captures pass six theme/viewport checks each, plus zoom/pan/Fit where
  supported. Final whole-factory and crossing-focus renders were personally viewed.

`compact-cases.json` lists all inputs. Reproduce the compact artifact with:

```sh
node docs/benchmarks/constructive-interconnect/compact-evidence.mjs \
  --cases docs/benchmarks/constructive-interconnect/compact-cases.json \
  --report-output target/compact-crossing-evidence-new.json
```

`compact-results.json` is also the **new complete incumbent report**: its
`selected_directory` points to the independently audited 3,120 witness. It preserves
core replay, completion-count distinctions, refinement provenance and source hashes
without duplicating all trial histories. Use it as the baseline in subsequent slices.

Final native artifact: `target/shared-results/bound-interconnect-closure-monotone-best`
(`.html` / `.png`). Core-only artifact: `structural-interconnect-monotone.html`.

Compared with the constructor design and repository policy: the new shared geometry
is embedded in private, immediately routed candidates, independently validated before
acceptance, and promoted only by complete whole-factory score. Global proof claims,
Rust cutover, recipe-breadth completion and product readiness remain explicitly out of
scope for this milestone. The broader goal stays active.
