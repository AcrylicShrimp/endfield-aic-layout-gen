# User screenshot reconstruction

Status: reconstructed geometry confirmed by the user; not optimized or operationally verified.
First case: medium-capacity Valley battery.

## Contract

Input: an explicitly transcribed case JSON, runtime recipe/facility/item/transport/component
catalogs, the source building table for a non-production power annotation, and the existing
independent witness validator. Screenshot coordinates are observations, not new game rules.

Output: immutable case geometry, unit-cell witness, projected catalog ports, per-route counts,
independent partial-geometry validation, and a native grid rendering. No router, placement
search, compaction or optimizer may run. No missing external input/output is fabricated.
Recipe utilization is a declared comparison assumption, not evidence of observed operation.

The source screenshot has no visible grid. Its inferred origin is approximately (112, 120)
pixels with a 72-pixel cell pitch. Seven 3x3 buildings and one rotated 6x4 building fit this
lattice. Pixel calibration is approximate; no pixel-perfect projection claim is made.
Power equipment is outside the normalized production-facility catalog and witness validator.
Its inferred 2x2 footprint is read from the source building table and shown only as an
annotation. Check its overlap separately; do not claim power connectivity or coverage.

Failures have code `reconstruction-input` or `reconstruction-validation`, severity `error`,
stage `screenshot-reconstruction`, a message and entity when available. Invalid catalog IDs,
poses, reused ports, nonorthogonal routes and mismatched endpoints must not be repaired.
The validator remains authoritative for transport and recipe-margin legality. Success means
the visible reconstructed subgraph validates, not that the screenshot proves a full factory.

## Reference collection

The user requested several independently designed in-game recipe layouts before changing
the optimization approach. Preserve each reference as supplied, distinguish observation
from interpretation, and discuss recurring patterns and exceptions before implementation.
This case must not introduce a new optimizer rule or become a production fallback.

Completion criteria: reproduce the visible poses and seven routes, independently validate
the covered subgraph, inspect an actual browser rendering against the screenshot, and report
remaining uncertainty. The source screenshot is not committed; retain its SHA-256 in the
case report. Generated artifacts belong under `target/`.

Activity log:

- 2026-09-06: Began manual grid transcription of the user's first battery screenshot;
  production optimization and multi-recipe construction experiments remain paused.
- 2026-09-06: User confirmed the rendered diagram is accurate. User also requested that power
  equipment remain visible without introducing power supply into planning constraints.

## Confirmed reconstruction

Bounds: 10x15 cells (150). Eight production facilities occupy 87 cells; the seven disjoint
visible belt connections occupy 32 cells. The inferred power annotation occupies four cells.
No splitter, converger or bridge appears. The five packager input lanes remain independent.
The visible paths have seven bends when facility-port arms are included; this is explicitly
not the optimizer's segment-only turn score. The last grinder's short up-then-down connection
is preserved rather than straightened or judged as an optimization defect.

The independent validator passes the covered geometry. At assumed full utilization, the only
six pending recipe terminals are two 0.5/s iron-ore inputs, three 0.5/s originium-ore inputs,
and the 0.1/s battery output. No internal recipe input remains pending. This validates the
transcription under the catalogs, not actual screenshot operation or complete external wiring.

Source screenshot SHA-256: `149c197ba0e0d200cc729afe934538ca4f87abfdeab4871f1b9815f02ef22abf`.
Witness SHA-256: `5c313c90670156cba95fcc0e6fe3954b43532c2dd370fb7a06550ca9b7b39480`.
Artifacts: `target/user-medium-battery-01/{witness,detail,validation,report}.json` and `index.html`.
Four model tests and all 279 currently discovered constructive/reconstruction Node tests pass.
`cargo check --workspace` and `cargo test --workspace` pass (524 tests, two ignored).
Native browser pan/zoom/Fit checks pass six viewport/theme combinations. The conversation
rendering's geometry was compared exactly to the validated witness; four browser cases at
736/360 pixels in light/dark themes contain all seven routes without clipped labels or overflow.
Native full view and conversation light/736 and dark/360 images were personally inspected.

Reproduce from the repository root, using a fresh output directory and an explicit screenshot path:

```sh
node docs/benchmarks/user-layout-reconstruction/run.mjs \
  --case docs/benchmarks/user-layout-reconstruction/medium-battery-01.json \
  --source-image /absolute/path/to/codex-clipboard-91111fb0-9437-4616-88bc-abec7ca8e410.png \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --buildings data/game/source/table-cfg/FactoryBuildingTable.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/user-medium-battery-replay
```

The reconstruction does not alter production APIs, catalog data or optimizer policy. Pending
uncommitted multi-recipe experiments are not part of this work unit.
