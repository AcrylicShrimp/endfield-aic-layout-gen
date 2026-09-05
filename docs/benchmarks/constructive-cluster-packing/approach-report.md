# Shared approach columns: complete-factory footprint

Status: verified improvement slice, not final compactness or full-goal completion.

## Outcome

The complete original heavy-xiranite factory improves from **107x88 (9,416)** to
**99x85 (8,415)**, a **10.63% area reduction**. Physical transport tiles fall 2,242 -> 2,056;
turns rise 257 -> 261. All 59 original facilities, 131 recipe terminal margins and seven original
external obligations remain independently validated. The component count stays fifteen and is
diagnostic only. More turns are acceptable because used area takes priority.

## Shared algorithm, not fixture-specific geometry

`orderedApproach` now shares a bend column between lanes whose closed vertical intervals do not
overlap. Overlapping upward lanes require increasing columns; overlapping downward lanes require
the reverse order. Opposite-direction intervals are disjoint under strict endpoint ordering.
Longest-path ranks attain the minimum interior-column count for this fixed monotone,
single-vertical-segment route family. Straight lanes need no interior column. Touching intervals
must be separated because sharing even a cell would be illegal.

See `../constructive-auto-interface/approach-contract.md` for the contract and pairwise proof.
The polyline-map interface is retained; the obsolete one-column-per-lane algorithm is removed.
Face separation is checked against the actual required depth instead of total lane count.
No arbitrary-route, full-factory or global area optimum is claimed.

With retained old cyclic blocks, the large connected region falls 74x85 -> 64x85 and the small
region falls 39x43 -> 38x43. The large first frontier's twelve target approaches need only five
bend columns; two lanes are straight. The common helper also serves cycle construction, so all
eight cyclic blocks were regenerated, not merely assumed unchanged. The largest block falls
38x26 -> 35x26 and 324 -> 301 tiles. The other seven keep their dimensions and tile counts.
Using those new blocks reduces the large connected region further to 61x85.

This is not a Rust migration. Catalog ports, capacities, recipe allocations, component arms,
collision rules and validation semantics are unchanged. Compared against the constructive
planner design: connected closed-input expansion and complete-incumbent retention are intact;
no global exact solver or caller-supplied production canvas is introduced.

## Experiments

| Inputs and search | Packings / closures | Complete closures | Elapsed | Selected area |
| --- | ---: | ---: | ---: | ---: |
| Retained cycles, broad nesting | 12 / 24 | 1 | 46.677 s | 8,858 |
| Retained cycles, radius two | 15 / 30 | 16 | 64.919 s | 8,585 |
| Retained cycles, radius one | 7 / 14 | 8 | 32.898 s | 8,585 retained |
| Regenerated cycles, broad nesting | 12 / 24 | 8 | 40.723 s | 8,415 |
| Regenerated cycles, radius two | 15 / 30 | 11 | 57.112 s | 8,415 retained |

These are full experiment times including multiple candidate closures, validation and artifact
output, not one generation latency. The first broad run overlapped regression/audit work; do not
interpret differences between these runs as controlled speed comparisons. Cycle regeneration
took 91.6 ms for candidate construction and 608.3 ms overall, excluding catalog load and build.
The final winning relative offset is (30, -40), selected in `approach-fresh-matrix.json`.
Use that report for further local refinement; `approach-final-matrix.json` retains its incumbent.

The five frozen packing reports correspond to `target/approach-columns-{nesting,refine,final,
fresh-nesting,fresh-refine}`. Every recorded packing source hash matches the current sources.
The regenerated block inventory is `../constructive-cycle-blocks/approach-matrix.json`.
Earlier incumbents and reports remain intact. Output geometry is accepted only after independent
validation of both candidate closures and the complete result selected by the parent process.

## Verification and visuals

- 63 related Node tests pass. For all 3,136 ordered three-lane endpoint pairs, tests independently
  expand unit cells, verify collision freedom at the generated minimum separation and brute-force
  every smaller positive bend-column assignment. Separate cases cover touching intervals, column
  sharing, input-array determinism, nonmutation, straight lanes and duplicate IDs.
- All 64 interface audit expectations pass, including thirteen recipe cases, missing-component
  controls and belt/pipe split/merge primitives. The combined local fixture is now 19x37 with 172
  tiles; eight-consumer and twenty-four-consumer fixtures are 26x57 and 58x161 respectively.
  These are local recipe interfaces, not additional complete complex-factory tests.
- All 32 regenerated cycle candidates and eight cycle audit controls pass. Eight final complete
  factory audit controls pass, including altered rates, omitted exports and fake external supplies.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing ignores.
  No Rust files or dependencies changed. Geometry validation uses the existing release probe.
- Personally inspected actual full-layout renders after retained-block nesting/refinement and
  regenerated-block nesting/refinement, plus the combined local fixture and largest cyclic block.
  Browser checks cover six viewport/theme combinations and full-factory zoom/drag/Fit behavior.
  The final refinement retains the same witness and produces an identical screenshot hash.

Final page: `http://100.114.24.86:8765/bound-interconnect-closure-shared-columns-final.html`.
Image: `target/shared-results/bound-interconnect-closure-shared-columns-final.png`.

## Remaining bottleneck and next direction

The full render still has a large upper-left void and long horizontal intervals between producers
and downstream chains. Common bank faces and whole-region translation remain stronger restrictions
than physical collision avoidance. Whole-grid empty-strip removal cannot close gaps containing
unrelated events at different rows. Narrower partial packs also do not guarantee smaller complete
closures; the valid complete incumbent must remain the comparison unit.

Next investigate horizontal compaction of a concrete validated embedding using difference
constraints: retain facility rigidity, vertical-segment alignment, terminal attachment and the
existing per-row obstacle order while allowing unrelated columns to move independently. A
longest-path formulation may provide a minimum-width result for that explicitly fixed-order
neighborhood, with independent validation and no global optimum claim. Include external-boundary
semantics and crossing/component identity in the contract before implementation. This is proposed
work, not implemented evidence. Keep the complete 8,415-area incumbent. Final acceptable compactness,
additional complete recipes, cleanup and the recipe/rate-to-layout web workflow remain unfinished.
