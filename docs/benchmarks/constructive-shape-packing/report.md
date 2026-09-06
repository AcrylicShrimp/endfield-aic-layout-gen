# Occupied-shape packing and measured group boundaries

Status: complete factory improved; grouping policy remains experimental.

## Complete result

The independently validated heavy-xiranite incumbent improves from 65x60 / 3,900
area / 609 transport cells / 83 turns to **104x31 / 3,224 area / 775 cells / 119
turns**. Area decreases 17.33%; the increased wiring is accepted under the unchanged
area-first objective. All 59 facilities, 131 recipe margins and seven original external
obligations validate against the original graph, not merely the allocated snapshot.

Occupied XY area is 1,848; empty area is 1,376 (42.68%). The previous result had
1,727 occupied and 2,173 empty cells. Largest measured empty rectangle decreases
15x35 / 525 to 32x6 / 192. The new factory is long and rectangular, not square.
No square-aspect objective was introduced, and satisfactory final compactness is
not claimed. The older five-member reciprocal group and remaining returns still waste
space. Native full and partial renders were personally inspected.

## Shared-engine change

The composition engine now accepts explicit normalized region translations instead
of always constructing a right-aligned rectangular stack. Port assignment, structured
branch/merge/crossing construction and independent validation stay shared with the
control. No second router or Rust production path was introduced.

The new shape experiment keeps bounded portfolios of concrete source witnesses, then
searches next-region order, shape and integer translations using occupied-layer row
bitsets. Bodies and same layers cannot collide. Pending compatible item margins must
retain sufficient unused east-access output connections. This east-ray condition is
an interface-family restriction, not a universal game rule. Shape diversity is a beam
heuristic, not an exact solver or dominance proof.

Seven banks retain up to 12 source shapes and the larger retained bank has one. Every
input shape used by the experiment is independently checked against current catalogs
and graph margins. Logical allocation is held fixed. The current 8-wide beam examines
209,286 translations for the small cluster and 549,172 for the large cluster. The first
observation spent 0.459s and 1.290s respectively in shape enumeration. Run times below
are single observations, some under concurrent experiment load, not speedup claims.

## Experiment matrix

| Source composition / growth | Complete closure attempts | Best new raw complete score | Outcome |
| --- | ---: | --- | --- |
| Previous rectangular source stack, full growth | 11/24 | 5,400 / 1,036 / 133 | Historical control |
| Occupied packing, one shape per bank, full growth | 1/24 | 5,040 / 1,385 / 184 | Control; 3,900 incumbent retained |
| Occupied packing, 12-shape portfolio, full growth | 1/24 | 5,040 / 1,375 / 184 | New seed; 3,900 incumbent retained initially |
| Same portfolio, later growth cut at 60% fill | 0/24 | No complete candidate; at most 53 facilities | Structured partial failure |
| Rectangular sources, later growth cut at 60% fill | 0/24 | No complete candidate; at most 52 facilities | Structured partial failure |

The larger cluster's selected source packing uses the same locally best envelope sizes,
but interlocks the short strips with two-cell horizontal staggering and six-cell row
spacing. It places the old larger reciprocal block beside them. Alternative envelopes
are available, but they are not established as the cause of the area improvement. The
one-shape control also reaches raw area 5,040. Retaining shape alternatives saves ten
raw transport cells in this matrix; no stronger causal claim is supported.

Three rounds of existing transactional connected-group relocation and terminal-flexible
A* rerouting improve the new raw complete seed to 3,224 / 775 / 119 (1,068 candidate
attempts, observed 68.316s). The raw seed alone is worse than the prior 3,900 incumbent;
only the final validated whole factory is promoted. The current-source full-growth
rerun reproduces all selected cluster witness hashes and the complete seed SHA-256
`683cd488b42cc9cbdf4c4d7eed17b4422ff3fb32dbcd10c547278d9ea20390cd` exactly.

## User-proposed density boundary

Measured original structured-growth fill (occupied XY / used rectangle):

| Cluster | First joined frontier | Next frontier | Last frontier |
| --- | --- | --- | --- |
| Small | 14 facilities, 63.64% | 16 facilities, 57.99% | 18 facilities, 53.72% |
| Large | 27 facilities, 42.00% | 31 facilities, 40.39% | 35 facilities, 38.98% |

This confirms that the previous grouping policy blindly grows sparse rectangular
envelopes along short process chains. Fill is useful evidence for where to split.
However, physical wires can inflate fill without improving productive density; facility
XY occupancy is reported separately, and fill never replaces the area-first score.

The 60% experiment stops later growth at 14 and 27 facilities, preserving exact pending
outputs. It currently requires the initial shared-consumer join so the outer two-cluster
experiment remains comparable. Consequently, it does not require the first join to meet
60%, nor does it implement arbitrary SCC-seeded partitioning or one/two-facility lookahead.
Do not misrepresent this limited prefix control as a full test of the user's proposal.

## Visually verified next obstruction

In the rectangular-source, 60%-cut control, packing 2 reaches a valid 52-facility partial
factory but cannot add F06. Actual render inspection revealed F11 tucked immediately
west of F17. F11's three east output connection cells are **(61,3), (61,4), (61,5)**,
and every cell lies inside F17's footprint. F11's routed input comes around F17 from
below, so the input-only growth transaction accepts a visually trapped facility.

This is not a failed proof of the density approach. The post-packing growth engine
checks the just-added input but fails to preserve access for remaining output obligations.
The next gate is to reject such future-port obstructions before committing growth, then
repeat the fill-boundary controls. Only after that compare genuine SCC-seeded stop/grow
portfolios and short lookahead, using completed combined area. Avoid hardcoding a fill
threshold or silently treating every unused port as required.

## Verification and artifacts

- 107 related Node tests pass; new tests cover concave interlocking, alternative shapes,
  nonzero origins, independent layers, pending-item aggregation, ray access, fill counting,
  threshold equality, schema/ownership errors and shared explicit-offset composition.
- `cargo check --workspace` passes. `cargo test --workspace`: 524 pass, zero failures,
  two existing ignores. No Rust edits or dependencies.
- Eight independent complete-factory audit controls pass against the original graph.
- Full, partial, obstruction-focus and refined native PNGs inspected personally.
  Each browser capture passes six theme/viewport checks plus native zoom/pan/Fit checks.
- `matrix.json`: current-source 12-shape full-growth exploration, including partial
  candidates, search counts, source hashes, validation and retained incumbent.
- `fixed-control.json`: one-shape control; `fill-control.json` and
  `shelf-fill-control.json`: threshold controls with exact rejected growth records.
- `full-matrix.json`: final connected-group improvement and complete witness location.
- `full-controls.json`: eight independent audit controls.

Rendered complete result:
`target/shared-results/bound-interconnect-closure-shape-refined.html` and `.png`.
Rendered obstruction:
`target/shared-results/bound-interconnect-closure-fill-cut-obstruction.html` and
`bound-interconnect-closure-fill-cut-obstruction-focus.png`.

The constructor-design invariants remain intact: private routed candidates, independent
acceptance and complete-incumbent preservation. This is isolated research tooling; the
broader goals of robust compact grouping, more recipes, cleanup and product workflow
remain open. No global minimum or factory infeasibility claim.
