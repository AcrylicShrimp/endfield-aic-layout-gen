# Reciprocal processors and complete-factory integration

## Outcome

The complete 59-facility factory improves from 73x54 / 3,942 area / 1,246 transport
tiles / 162 turns to 65x60 / 3,900 area / 609 tiles / 83 turns. Area improves only
1.07%, despite a 51.12% tile reduction. Area remains the primary objective; the
smaller-wire 4,352-area candidate was not selected. All 131 recipe terminal margins
and seven real external obligations validate against the original graph, not only
the reallocated intermediate graph. The overall product goal remains incomplete.

## Processor attachment

The first integration applies each core's explicit allocation delta, preserving exact
whole-graph per-node/per-item supply and demand. Stale, duplicate, cross-commodity,
invalid-rate and unbalanced deltas are rejected. The same deltas are applied to the
combined output snapshot before independent region and complete-factory validation.

Shared structured composition initially attaches seven processors at 11x15 / 165 area,
eight belt cells and no logistics components. All 168 generated attachment candidates
validate. The former five-member reciprocal core plus its two processors remains as
one validated retained region; no shared supplier is duplicated or dropped.

Visual inspection exposes the structured engine's east-facing supply assumption. It
cannot attach a processor to the horizontal north/south/north strips. A direct family
therefore enumerates free supplier ports on any face, east-flow processor ports, and
shared or adjacent connection cells. Actual body/transport occupancy and a usable
eastward processor export are checked. The processor may touch an unused facility face.

The best seven replacements become 18x7 / 126 area, seven belt cells, no components.
The processor consumes through a shared corner cell; a full extra straight approach
cell is unnecessary. These are validated four-facility regions with one downstream
processor-output margin pending, not complete factories or globally minimum modules.

## Full graph experiments

| Experiment | Complete closures | Complete area / tiles / turns | Elapsed seconds |
| --- | ---: | --- | ---: |
| East-facing attachment, nested clusters | 2 / 24 | 6,643 / 1,355 / 139 | 45.269 |
| Above, three joint group rounds | Complete retained | 4,624 / 914 / 108 | 48.116 |
| Above, three further group rounds | Complete retained | 4,148 / 876 / 109 | 41.532 |
| Direct attachment, nested clusters | 11 / 24 | 5,400 / 1,036 / 133 | 38.722 |
| Direct seed, single-facility continuation | Complete retained | 4,352 / 603 / 71 | 17.095 |
| Direct seed, group continuation | Complete retained | 3,900 / 609 / 83 | 38.560 |

Continuation families ran concurrently and have different work counts. These timings
are diagnostic observations, not controlled speed comparisons or generation guarantees.
The complete old incumbent is retained until a complete candidate has a better score.
The first two new families never replace the prior 3,942-area factory.

Frozen final evidence is attachment-matrix.json, attached-graph.json,
full-packing-matrix.json, full-matrix.json and full-controls.json. Intermediate evidence
remains under target/reciprocal-{attached,full-nesting,full-groups,full-refined,
direct-attached,direct-full,direct-single}. Reports identify their own source/input hashes.

## Validation cost

The exhaustive direct-attachment control generates and validates 3,192 candidates;
all pass. It takes 37.849s total, with 3.473s in generation. Ranked validation uses
the exact same score and stable tie-break as final selection, checking each block's
best candidate first and continuing only on rejection. Candidates later than an accepted
winner cannot win this particular selection rule and need no per-candidate files.

Ranked mode validates seven candidates, skips 3,185 and separately rechecks all eight
selected regions against the combined graph. Total elapsed is 3.943s, generation 3.414s.
All eight selected witness hashes and the complete emitted graph snapshot are identical
to the exhaustive run. These are single observations; the preserved selection and reduced
validation/file count are established, not a universal throughput multiplier.

Exhaustive mode remains available through --validation-policy exhaustive. The ranked
control is the current default. It preserves the current one-winner-per-block selection;
it does not claim that discarding other block shapes is optimal for global packing.

## Visual findings and next contract

Personally inspected native images of the first attached module, its full raw factory,
group-refined factory, direct attached module and final complete factory. Browser checks
cover six viewport/theme combinations and full-layout zoom, pan and fit interactions.

The final witness occupies 1,727 distinct XY cells inside its 3,900-cell bounding box.
The largest empty rectangle is 15x35 (525 cells), with other broad left/lower gaps.
The old shared five-member cycle still contains long return geometry. The complete
factory is valid but cannot yet be called satisfactorily compact.

The user's final visual feedback is important: a locally small nonrectangular module
can pack badly. Current inner composition uses stack() with each module's rectangular
bounds and advances by height + 1, then places an east-side interface. Only the outer
two-cluster nesting pass considers actual occupied cells. This is a real restriction,
not evidence that the visible dead space is unavoidable.

Next, retain several module shapes/processor positions and compare occupied-cell
interlocking placements at the inner composition level. Module boundaries should be
ownership boundaries, not solid obstacles. Preserve selected port access, restore
all inter-module connections, and rank the complete combined footprint. A locally
larger module may win globally. Do not keep only each block's minimum-area shape or
substitute more fixed-order compaction for this change. Also address the shared
five-member reciprocal region without dropping rates or duplicating suppliers.

Interactive final result:
http://100.114.24.86:8765/bound-interconnect-closure-reciprocal-current.html

Images:
target/shared-results/bound-interconnect-reciprocal-direct.png and
target/shared-results/bound-interconnect-closure-reciprocal-current.png.

## Verification and policy

100 related Node tests pass. New tests cover exact graph-delta conservation/rejection,
shared-corner attachment, multiple-input exclusion, occupied supplier ports, east-flow
processor orientation, collision-free normalized geometry and frozen input isolation.
Eight full-factory audit controls pass using the original production graph. cargo check
--workspace passes; cargo test --workspace: 524 pass, two existing ignores. No Rust
files, game data, production APIs or exact-solver dependencies changed.

This composes already routed atomic regions and validates each covered expansion;
it does not defer all physical routing to a global facility-only placement phase.
Transactional acceptance, natural used bounds and area-first ranking match the
constructive planner design. No global infeasibility, minimum-area or completed
web-service claim is made.
