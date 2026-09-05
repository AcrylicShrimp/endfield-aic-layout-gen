# Layered crossing footprint experiment

Status: complete verified slice; the full factory-quality goal remains active.

## Contract and implementation

The shared permutation geometry now schedules independent adjacent swaps in the same columns.
It preserves the original order of swaps sharing a position, catalog bridge geometry, rates and
resource guards. A longest-path schedule minimizes depth for that fixed dependency graph only;
it does not prove minimum permutation width or minimum factory area. See
`../constructive-interconnect/contract.md`. Serial placement is removed rather than retained as
an internal compatibility mode. Component count is diagnostic, never an optimization priority.

The actual large cluster's four crossings occupy two stages instead of four: swaps at positions
3 and 7 share the first stage, and positions 2 and 8 share the second. Raw crossing-core width
falls from 24 to 12. After event-free strip contraction, the first frontier width falls 72 -> 66
and the completed large cluster falls 80x85 -> 74x85, with 1,308 -> 1,236 transport tiles.
The smaller 39x43 cluster is unchanged. All four crossings are retained.

## Complete-factory results

| Experiment | Packings / closure attempts | Elapsed | Selected complete area |
| --- | ---: | ---: | ---: |
| Broad occupied-cell nesting | 12 / 24 | 44.802 s | 9,592 |
| Radius-two refinement | 21 / 42 | 81.339 s | 9,416 |
| Final radius-one check | 9 / 18 | 41.188 s | 9,416 (incumbent retained) |

These are whole experiment times including validation and multiple closures, not one generation
latency. Frozen reports are `layered-matrix.json`, `layered-refine-matrix.json` and
`layered-final-matrix.json`. Candidate details remain in their recorded target directories.
The final report's source hashes match the current shared geometry and interface sources.
The initial broad report predates adding those shared dependencies to the provenance list;
both refinements explicitly record them.

The incumbent improves from 112x88 (9,856) to 107x88 (9,416): **4.46% less area**.
Physical tiles fall 2,299 -> 2,242; turns rise 256 -> 257. Fifteen components remain.
All 59 original facilities, 131 recipe terminal margins and seven original external obligations
are independently validated. No missing recipe is replaced with an undeclared external supply.

The winning relative cluster offset is (25, -45). Moving one column closer to (24, -45) still
completes, but closure produces 107x89 (9,523), so it loses on area. Offset (24, -46) yields
106x89 (9,434), also worse despite using fewer tiles (2,235). Partial packing area and component
count are not substitutes for complete-factory area. No local or global optimum is claimed.

## Verification and visual inspection

- 62 related Node tests pass. The exhaustive permutation test independently checks all 873
  permutations through six lanes for cell legality, inversion count and dependency depth.
  The eight-test interconnect suite passes again after adding the depth assertions.
- `cargo check --workspace` and `cargo test --workspace` pass: 524 tests, two existing ignores.
  No Rust code or dependencies changed; validation uses the existing release witness probe.
- All eight independent positive/negative audit expectations pass (`layered-controls.json`).
- Actual full-layout and detail renders were personally inspected after broad nesting,
  refinement and the final check. The final image visibly retains substantial empty approach
  space; this is an improvement milestone, not satisfactory final compactness.
- Compared with the constructive planner design and repository policy: connected closed-input
  expansions and complete-incumbent retention remain intact; no global exact solver, fixed
  production canvas, speculative port rule or implicit crossing is introduced.

Final tailnet page: `http://100.114.24.86:8765/bound-interconnect-closure-layered-final.html`.
Native screenshot: `target/shared-results/bound-interconnect-closure-layered-final.png`.

## Next measured opportunity

`orderedApproach` still allocates a distinct bend column to each lane even when their vertical
intervals do not overlap. Global empty-strip removal cannot combine columns containing events
at different rows. Investigate column sharing with explicit interval-conflict constraints and
independent cell checks, then measure the complete factory again. Keep the 9,416-area incumbent
until an independently validated complete improvement exists. Additional recipes, final quality,
cleanup and the actual recipe/rate-to-layout web workflow remain unfinished.
