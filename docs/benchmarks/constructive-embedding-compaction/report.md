# Fixed-order complete-embedding compaction

Status: verified improvement slice. Full-goal completion and satisfactory final compactness remain open.

## Result

The complete original heavy-xiranite witness improves from **99x85 (8,415)** to
**91x75 (6,825)**: **18.89% less area**. Physical transport tiles fall **2,056 -> 1,789**;
turns remain 261 and components remain fifteen. All 59 original facilities, 131 recipe terminal
margins and seven original external obligations pass independent validation. No rate, recipe,
rotation, component arm or crossing rule changes.

The new experiment acts on a complete validated concrete factory. It suppresses constant-rate
straight interiors, retains rigid facility/port and perpendicular-segment equalities, and preserves
strict critical-point order in each perpendicular row. Unrelated coordinates can then move
independently. The resulting difference constraints admit a componentwise least integer solution,
which minimizes the axis extent for that explicitly frozen ordering/coincidence/topology family.
See `contract.md`. This is not a proof of global minimum area or minimum wire length.

The original complete witness certifies feasibility before solving. Explicit point, constraint-work
and expanded-segment budgets produce structured diagnostics. Each axis candidate undergoes
independent release-validator and exact original graph-coverage checks; only a complete better
area/tiles/turns tuple replaces the incumbent. Rejected or nonimproving candidates remain evidence.
The constructor and prior strip compactor remain unchanged: these are different improvement
neighborhoods, not compatibility shims or production solver fallbacks.

## Axis-order comparison and timing

| Order | Complete accepted score progression (area / tiles) | Final bounds | CLI elapsed |
| --- | --- | --- | ---: |
| x then y | 8,415/2,056 -> 7,820/1,958 -> 6,992/1,815 -> 6,916/1,802 | 91x76 | 1.033 s |
| y then x | 8,415/2,056 -> 7,425/1,902 -> 6,825/1,789 | 91x75 | 0.717 s |

Both runs stop after an unimproved axis pair. All ten attempted axis candidates validate, including
the nonimproving controls. The different final heights demonstrate order dependence, not a solver
error. The y-first result is retained in `matrix.json`; `xy-matrix.json` records the other order.
All recorded source hashes match the finalized implementation. Detailed candidate certificates,
geometry, validator streams and original-graph coverage reports remain in the recorded directories.

The final y-first run's two improving passes take 89.1 ms and 90.5 ms for model construction,
solving, lowering and detached output construction. The difference solvers themselves take 2.2 ms
and 4.3 ms: 112 groups/457 inequalities, then 152 groups/612 inequalities. Two nonimproving passes
take 53.5 ms and 77.3 ms. CLI elapsed includes input reads, incumbent/candidate validation and HTML/
JSON output, but excludes prior factory generation and final report serialization. These are single
observations, not controlled median latency estimates. No Rust compilation is included.

Reproduce with `node docs/benchmarks/constructive-embedding-compaction/run.mjs`, passing explicit
`--graph`, `--baseline`, all five catalog flags, `--validator`, `--axis-order yx` and a fresh
`--output-directory`. Baseline is `../constructive-cluster-packing/approach-final-matrix.json`;
graph and catalog/executable paths are recorded with hashes in `matrix.json`. Default round budget
is eight; width and height remain measured outputs, never caller feasibility limits.

## Verification and independent review

- 68 related Node tests pass. New tests brute-force 500 feasible three-variable difference systems,
  cover all 2,000 current runtime facility/rotation/port/axis combinations, and check reverse flow,
  rate transitions, obstacles, layer coincidences, component positions, external sides, nonmutation,
  detached output, axis idempotence and explicit resource-limit diagnostics.
- All eight independent full-factory positive/negative controls pass (`controls.json`).
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing ignores.
  No Rust files or dependencies changed. The existing release witness probe validates geometry.
- Independent proof/bug review confirmed the least-solution argument and reproduced all ten original
  axis candidates exactly, checking terminal rates/directions, unit segments, facility utilization,
  component attributes and external sides. The main agent independently reran the expanded catalog
  tests and current complete-factory runs. No geometry/proof bug was confirmed.
- Independent optimization/diagnostic review identified three actionable implementation issues:
  existing output directory failure handling, recursively copied certificate history, and omitted
  port-projection source provenance. Local regression verifies existing directories and sentinel
  files are untouched while stderr remains structured. Direct repeated calls reproduced history
  growth (1,039,596 -> 1,195,439 -> 1,350,289 serialized characters); history is now retained once,
  separately from the current certificate. The selected pretty JSON shrinks 1,890,741 -> 1,484,423
  bytes. Port-projection and renderer sources are included in provenance. The four y-first candidate
  witness hashes remain identical after these fixes. Large rigid-group lower bounds also avoid an
  argument-spread maximum. No reviewer claim was used as an unverified pruning premise.

The design was compared with the constructive planner policy: all construction remains connected
and transactional; this is a separately validated complete-layout improvement. No fixed production
canvas, global joint solver, implicit external material or unvalidated replacement is introduced.

## Actual visual inspection and next bottleneck

Personally inspected the actual final full render before and after review fixes. Browser checks
pass for six viewport/theme combinations plus zoom, drag and Fit. The page is
`http://100.114.24.86:8765/bound-interconnect-closure-fixed-order-final.html`; the native image is
`target/shared-results/bound-interconnect-closure-fixed-order-final.png`.

The render exposes a quality issue despite the improved complete score: the F55/F58 pair moves
far north while F54 stays lower, leaving a long local connection and a large void. Componentwise
least coordinates minimize extent but need not minimize wire length within that same extent.
This is not accepted as satisfactory final compactness. Both reviewers distinguish this limitation
from correctness; the overall tile count nevertheless improves.

Next keep the proven minimum axis extent fixed and optimize total retained-chain length subject
to the same rigid/order/boundary constraints. With directed chain orientation fixed, that cost is
linear in coordinate differences. A bounded integer threshold/closure formulation or equivalent
network optimization may solve this secondary objective exactly for the fixed neighborhood.
That is proposed work, not implemented or globally optimal evidence. Then compare axis orders
again and inspect actual outcomes. Keep the validated 6,825-area incumbent. Broader complete
recipes, cleanup and the actual recipe/rate-to-layout web workflow remain unfinished.
