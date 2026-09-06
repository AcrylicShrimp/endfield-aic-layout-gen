# Catalog-coherent facility rotation

## Outcome

The complete heavy-xiranite witness improves from 2,173 / 407 / 41 to
**2,173 / 395 / 38** (area / physical transport cells / turns). All 59 original
facilities and exact external margins remain independently validated. The 41x53
footprint does not improve. Use facility-rotation-results.json as the current
incumbent; the old witness is preserved in push-boundary-results.json.

The important distinction is between two neighborhoods: rotation combined with an
inward boundary group does not restore a complete moved factory, whereas rotating
one facility independently does. Do not interpret the former failure as evidence
against all rotations, or the latter success as minimum-area packing.

## Contract and implementation

Facility pose schema 4 adds an explicit absolute catalog rotation to selected
translation primitives. The transaction projects the non-square body, world ports,
terminal positions and retained endpoint-arm turns coherently from runtime data.
The validated parent is never pre-rotated. Shared owners are checked using actual
terminal displacements, not just body translations; non-detachable owners with
incompatible arm rotations are rejected in this neighborhood. Recognized isolated
zero-length pairs can separate without losing their directed endpoint identities.

One internal `anchorTurns` map replaces the component-only turn map. Components and
facilities now use the same directed-arm handling; rotation cannot accidentally
carry an old internal route or take the unchanged shortcut. Explicit port assignment,
vacant/cooperative domains and diagnostic overlays all project the same new pose.
No parallel rotated-parent path or compatibility adapter was introduced.

The deflection family optionally interleaves catalog rotations. The separate
`--move-family rotate` generates nonretained rotations at the existing origin and
cardinal offsets of one/two cells, screens the same collision/ownership/extent
constraints, and fairly ranks surviving facilities. It retains the existing four
primitive/sixteen facility/eight released bridge/eight new crossing limits. Its
single-facility proposals release incident bridges plus bridges covered by the body.
The complete-candidate validation and area-first score gate remain unchanged.

## Controlled experiments

All runs below use joint mode, terminal-flexible endpoints, downstream junctions,
eight new crossings, incident/body bridge release, vacant-port repair and independent
layer compaction. The validator is the existing release build. Times are observed
optimization-batch durations, with some concurrent work; they are not constructor
latencies or isolated performance measurements.

| Neighborhood | Poses screened | Routed positions | Attempts | Result | Observed time |
| --- | ---: | ---: | ---: | --- | ---: |
| Deflection, retained rotation | 3,709 | 142 | 2,046 | Unchanged 407 / 41 | 6.03s |
| Deflection, catalog rotation | 15,404 | 256 of 558 survivors | 3,898 | Unchanged 407 / 41 | 11.62s |
| Standalone rotation, three rounds | 4,779 | 1,171 | 11,833 | 395 / 39 | 44.97s |
| Standalone rotation, continuation | 3,186 | 776 | 7,780 | 395 / 38 | 31.22s |

The deflection pose streams exhaust before their 16,384 cap. Catalog mode selects
183 actually rotated positions and 73 retained-rotation positions; neither control
reaches a complete facility-movement candidate. Their unchanged witness hashes
match the 407-cell parent exactly. Final control reports are
target/facility-rotation-final-{retained,catalog}/report.json.

Standalone rotation examines 1,593 poses per round (59 facilities, three nonretained
rotations, nine translations). Every geometry survivor enters routing at the 512
selection limit. Per-round survivors are 399, 398, 374, 392 and 384. The first run
ends on its three-round cap; the continuation completes two of its five allowed
rounds and stops on an unimproved neighborhood. Twelve selected candidates and
sixteen additional unimproved audits across these two runs all pass full validation.
Unselected generated candidates are not implicitly called validated.

F01's 90-to-180 degree rotation first exposes a better pipe arrangement. F03's
90-to-0 degree rotation plus port repair and the existing compactor improve the
lower product chain. F07 temporarily rotates and then returns to 90 degrees at a
different position, removing a final turn. Final F01 and F03 rotations differ from
the original parent; postprocessing also changes several lower-region positions.
Thus the overall improvement is rotation **plus existing rerouting/compaction**,
not a claim that changing an angle alone reduces twelve tiles. Round-ending scores
are 403 / 40, 399 / 41, 395 / 39 and 395 / 38. The temporary increase in turns is
legal because physical tile count has higher priority.

An earlier auxiliary 3,186-attempt fixed-parent single-facility matrix discovered
F01's 403 / 40 candidate (target/facility-rotation-smoke/report.json). The integrated
runner above supersedes it as the reproducible selection path. A 35-trial priority
replay of rejected catalog-deflection stage 2793 finds no candidate; the F25 proposal
is shown separately from validated geometry. The relaxed chain-61 path ignores
wiring and component arms and is explicitly not valid routing.

## Verification and visual inspection

- 251 Node tests pass, including independent non-square quarter-turn coordinates,
  catalog/ID/budget errors, immutable inputs, rotated collision/bounds checks,
  retained arm rotation, shared-owner displacement/turn agreement, detached directed
  pairs, projected port domains, deterministic standalone proposals and viewer data.
- cargo check --workspace and cargo test --workspace pass: 524 Rust tests, two
  existing ignores. No Rust, data catalog or production API changes.
- Two invalid manual CLI combinations reject before creating output directories.
- Eight original-graph controls and sixteen crossing mutation controls pass on the
  final witness. Broken connections, missing bridge ownership and changed margins
  cannot pass the complete-factory gate.
- Input and engine hashes for both integrated runs and both final deflection
  controls match the current source. Raw final witness SHA-256:
  492c4da7c72275215a35d423a9375d36c67a23e7915b66479b6aefe1d6ba384f.

Personally inspected the rejected rotated overlay, F01's first validated pipe
change, the final complete factory and its lower F03/F07/F12/F18 region. Native
pages pass six viewport/theme checks plus pan/zoom/Fit. Rotated F03 now receives
its furnace outputs on the lower side; F07 and F12 are staggered. The lower return
and conspicuous central empty region remain. No area gain or satisfactory final
compactness is claimed. Final viewer:
http://100.114.24.86:8765/bound-interconnect-closure-rotation-best.html.

Compared against AGENTS.md and the constructive planner design's Iterative
Improvement contract: every mutation/repair is private, unchanged validated regions
remain available, only complete better candidates are accepted, and current extent
is a local neighborhood, not a production limit. This JavaScript experimental slice
does not cut over production or use the global exact solver. The next area-reduction
experiment can reuse the coherent pose API on the new 395-cell parent; do not repeat
old 407-parent failures as if they prove anything about the changed geometry.
Recipe breadth, production integration, cleanup and the web workflow remain open.
