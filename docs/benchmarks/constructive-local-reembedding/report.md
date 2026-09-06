# Free-path rerouting and transactional facility relocation

Status: complete heavy-xiranite improvement verified; final space quality is still inadequate.

## Result

The retained complete factory improves **91x75 -> 83x69**, area **6,825 -> 5,727 (-16.09%)**.
Physical transport tiles fall **1,603 -> 1,321 (-17.59%)**; turns fall 261 -> 195. All 59 original
facilities, 131 recipe terminal margins and seven actual external obligations remain independently
validated. The fifteen logistics components are unchanged in identity and count; count is not an
objective. All facility rotations and utilizations are preserved.

The difference from the previous compactor is substantive: ordinary bends are no longer protected
anchors. A* searches new unit-grid paths between logical anchors, and a relocation transaction can
remove every wire intersecting the new body, not only the moved facility's own wires. Failed
transactions leave the entire valid incumbent intact. Post-compaction can exploit the changed
relative order and route shape. Neither the old template order nor old bounding rectangles of
submodules are relocation constraints.

## Actual experiment sequence

| Experiment | Attempts | Accepted improvements | Final area / tiles | Elapsed |
| --- | ---: | ---: | ---: | ---: |
| Fixed-path A* plus post-compaction control | 96 | 32 | 6,278 / 1,424 | 17.571 s |
| Screened relocation, two rounds | 256 | 2 | 6,020 / 1,392 | 13.049 s |
| Joint reroute/move continuation | 448 | 4 | 5,796 / 1,351 | 40.776 s |
| Joint continuation with unchanged-path screen | 448 | 2 | 5,727 / 1,321 | 19.971 s |
| Current-source verification neighborhood | 224 | 0 | 5,727 / 1,321 | 7.227 s |

These are bounded experiment times on an existing complete witness, not from-scratch factory
generation latency. Each includes many candidates, validation, post-compaction and diagnostic/HTML
output; no Rust rebuild is timed. The fixed-path control does not propose arbitrary facility moves,
but its post-compaction can move coordinates. Do not call it a strictly fixed-placement final layout.
Each successive report records its input baseline and exact source hashes; early experiments are
historical revisions, not all current-source replications. Frozen reports are `fixed-control.json`,
`move-matrix.json`, `joint-matrix.json`, `improvement-matrix.json` and final `matrix.json`.

Final verification uses current source hashes and independently rechecks the complete incumbent.
Its 224 attempts comprise 88 unchanged paths, 14 unimproved candidates and 122 routing rejections.
The explicit 64-position shortlist, two routing orders and fixed component/port anchors were not
enlarged; no claim that further relocations or all path combinations are exhausted is made.

The first relocation trial spent its entire 32-position shortlist on collisions or disconnected
approaches. Geometric screening now happens before the shortlist budget: fixed bodies, anchors,
components, translated terminals and shared-terminal limitations are checked before ranking.
Only actual search candidates consume the selected-position budget. Future chain attachment arms
are reserved during multi-chain rerouting. The unit tests exercise displacement of a nonincident
pipe by a moved catalog facility, as well as immutable rejection and margin preservation.

## Visual interpretation, not just validator success

Personally inspected all complete stage renders and the same F35/F34/F47 region before and after.
The old return path has redundant outward/upward/inward doglegs; A* removes these. Relocating F47
and F44 to the opposite side of their producers shortens two return channels at the cost of one
forward U path. The cycle itself remains necessary; the previous physical shape was not.
F03 moves beside the head processing facilities. The images show real catalog ports and physical
paths; no explanatory drawing substitutes for the actual witness.

However, this is **not satisfactory final compactness**:

- The upper-left largest empty rectangle is still **28x32 (896 cells)**, versus 29x34 previously.
- Actual occupied XY cells total 2,339 within 5,727 area, leaving 3,388 unused cells (59.16%).
  Independent belt/pipe overlap is counted once here. This is different from the transport-layer
  tile objective. Facilities alone occupy 1,133 cells.
- F01 moves north, shortening its fixed-boundary approach but leaving long connections to the
  rest of its process. It is accepted by the configured area-first score, not presented as a
  visually natural arrangement or a solution to process locality.
- Some nested returns, central junction spacing and long inter-region links remain. Individual
  facility moves preserve too many surrounding terminals/components; terminal-distance ranking
  can miss coordinated moves that reduce the overall bounding box.

Final native full render: `target/shared-results/bound-interconnect-closure-local-current.png`.
Same-region comparison: `bound-interconnect-closure-minimum-length-final-focus.png` versus
`bound-interconnect-closure-local-current-focus.png` in that directory. The actual interactive
page is `http://100.114.24.86:8765/bound-interconnect-closure-local-current.html`.
Six viewport/theme checks plus full-factory zoom/drag/Fit pass for the stage pages.

## Verification

- 83 related Node tests pass. The new path oracle matches independently enumerated simple paths
  for all 1,024 obstacle masks on a small grid, with explicit directed-arm and work-budget tests.
- Actual-cycle tests cover all eight runtime-derived blocks, complete chain/edge-rate partition,
  immutable input/output separation, unchanged margins and untouched networks. Geometry-only
  synthetic tests are explicitly separated from actual recipe-validation evidence.
- All eight complete-factory audit expectations pass (`controls.json`), including missing export,
  fake external supply, duplicate facility, changed utilization and a broken pipe segment.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing ignores.
  There are no Rust, game catalog, dependency or production API changes.
- Every selected candidate is independently geometry- and original-graph-validated. No candidate
  with incomplete rerouting or failed validation replaces the incumbent.
- Initial test snapshots exposed null-prototype versus structured-clone prototype differences,
  not a witness mutation; tests now compare detached DTO snapshots. A deliberately obstructed
  cyclic move showed why affected-chain diagnostics must include not-yet-attempted chains after
  an earlier failure; the full scheduled set is now retained. No validator semantics were relaxed.

Compared with the constructive planner design, improvements remain atomic complete-witness
transactions, with area then transport tiles then turns. A* uses the current extent as a local
neighborhood, not a production feasibility ceiling or game limit. This remains isolated JS
research pending demonstrated efficacy across broader complete requests.

## Next gate

Use the visible large void to drive **connected multi-facility relocation**, carrying required
internal junction anchors and rebuilding the interface, with movable external approach positions
where original endpoint identity/side semantics permit. Compare two-to-four-facility groups and
small cyclic groups rather than merely increasing the single-facility shortlist. Include area
opportunities that temporarily worsen endpoint-distance or transport length. Preserve the complete
5,727-area incumbent until a full candidate validates and improves. Final compactness, additional
complete complex recipes, cleanup and the real recipe/rate-to-layout web workflow remain open.
