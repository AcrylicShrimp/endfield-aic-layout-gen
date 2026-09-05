# Port-centered supplier-group placement

## Result

All 59 Heavy Xiranite facilities are re-placed with one-cell clearance and legal catalog rotations.
The footprint changes from **52 x 98 to 52 x 79**, area 5,096 to 4,108 (**19.39% smaller**).
These are facility-only bounds, not a routed factory score. All 89 internal and seven external
logical requirements are preserved; physical routing and factory validation remain false.
The logical belt center-Manhattan span increases from 1,004 to 1,015, and 18 non-forward logical
requirements remain. Thus the smaller rectangle and corrected local group do not mean every
connection became closer. All stable instances retain their F labels and rotations in this run.

The motivating group now has the following normalized coordinates (tile origins):

| Facility | x | y | Size | Relationship |
| --- | ---: | ---: | --- | --- |
| F09 | 29 | 9 | 5 x 5 | Consumer |
| F14 | 25 | 8 | 3 x 3 | Upper supplier |
| F15 | 25 | 12 | 3 x 3 | Lower supplier |

The suppliers' combined vertical interface span is centered on F09's input span. Both have
compatible output/input connection cells in the same row and column, separated by the one empty
tile between facility rectangles. Their minimum endpoint distances are both zero; a shared
connection cell still requires a physical transport tile. This is geometric alignment, not a
capacity, route or port-assignment proof. F14 is now above F15; labels for these instances did not
change. The old geometry instead aligned F14's top edge with F09 and displaced F15 four cells north.

## Changed contract

Keep runtime catalog orientation (active belts before pipes), graph discovery priorities, north
pipe preferences and one-cell clearance. At each frontier, collect newly discovered exclusive belt
suppliers whose internal outputs all serve the current consumer and whose belt interfaces face
east-to-west. Pack them in frontier order with one empty row between siblings and translate the
whole group, never resolving a sibling collision by moving only one member.

For each member, derive compatible port connection-cell pairs with already-placed neighbors.
Belt relations determine alignment if present; otherwise use pipe relations. Candidate group
translations include port-alignment rows, floor/ceiling of interface-span centering, obstacle
clearance edges and north-pipe preference boundaries. Rank legal candidates by:

1. North-input pipe preference violations.
2. Pipe-rich branch vertical inversions against existing obstacles in the same clearance column.
3. Sum of per-requirement minimum compatible endpoint Manhattan distances.
4. Maximum of those per-requirement minimum distances.
5. Error between the supplier and consumer interface-span centers, then northern tie-break.

This is a finite deterministic heuristic, not exhaustive translation, ordering or port assignment
optimization. Compatible pairs here mean catalog direction and transport, not capacity-tested
assignments. Different requirements may favor the same port; neither approach accessibility nor
simultaneous routing is established. When no compatible pair exists, use a recorded footprint-center
anchor instead of inventing ports; physical feasibility remains untested.

Individual placements use the same port-distance rule, replacing the old top-edge rule. This means
a one-to-one chain with offset ports can shift vertically to align the actual ports. Shared
producers and cyclic relationships remain individual placements considering existing neighbors;
entire upstream subtrees are not packed as rigid groups. Prior placements do not move. Later
obstacles can shift a group away from its ideal center, with group scores recorded explicitly.

Each decision adds `supplier_group`, `alignment_basis`, and group-level port-distance/centering
scores under `row_choice`. The viewer displays those fields. `row_choice.y` and `preferred_row`
remain the individual member's normalized row; score values apply to the shared group translation.
The report's policy explicitly describes the new ordering. No top-edge compatibility mode remains.

## Timing and verification

Twenty sequential fresh Node processes, using the release graph CLI, with no concurrent build/test
load and warm files: placement median **32.76 ms**, whole graph-to-JSON/HTML pipeline median
**214.57 ms**. Build time, browser loading and routing are excluded. The first sample is retained;
all twenty geometry hashes match. `timing.json` preserves every sample, input/binary hashes and
phase statistics. The previous north-placement median was 25.66 ms; this slice adds group/port
scoring and is not a placement-speed optimization.

- Fourteen Node tests pass. Regressions cover actual offset-port alignment, the 3x3 + 3x3 supplier
  group centered against a 5x5 consumer, whole-group obstacle displacement, shared-producer exclusion,
  shuffled-edge determinism, cycles, north preferences, invalid data and file-only CLI outputs.
- `cargo check --workspace` and `cargo test --workspace` pass: 517 Rust tests, two existing ignores.
  No Rust files changed and no release rebuild was needed. Existing tests were not removed.
- Manual CLI and stable-ID comparison preserve instance sets and all logical requirements.
- Browser checks at 1400/736/360 px in both themes pass, including default port rendering, group
  inspector text and no horizontal page overflow. Actual overview and F09/F14/F15 screenshots
  were inspected. Both applicable north preference status and unsupported mixed-face diagnostics
  remain explicit; do not infer routing from the picture.

## Reproduction and scope

```sh
node docs/benchmarks/constructive-placement-only/time.mjs NEW_DIRECTORY 20
node docs/benchmarks/constructive-placement-only/compare.mjs \
  docs/benchmarks/constructive-pipe-north/heavy.json NEW_DIRECTORY/00-placement.json NEW_COMPARISON_JSON
```

`heavy.json` is the new frozen proposal; `comparison.json` maps old/new instances and geometric
metrics. Historical reports and pages are unchanged. The new page is served through the existing
tailnet server at `http://100.114.24.86:8765/placement-only-supplier-groups.html`. Actual rendered
images are `target/shared-results/placement-only-supplier-groups.png` and
`target/shared-results/placement-only-supplier-groups-detail.png`.

Compared against AGENTS policy and `docs/designs/2026-09-02.00-constructive-layout-planner.md`, this
is only the user's explicitly approved placement-only diagnostic. Production remains connected
transactional growth. No production constructor, router, game catalog or exact solver changes.
The next step is to review this placement and mechanically route it under the established
placement/nearest-connection/nearest-port order, then inspect actual remaining failure patterns.
