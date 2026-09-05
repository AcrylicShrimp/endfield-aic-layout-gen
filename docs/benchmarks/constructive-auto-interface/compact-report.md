# Area-first structured interface compaction

## Outcome and scope

The combined eight-facility fixture shrinks from 73 x 32 to 24 x 37: area 2,336 to 888,
a 62.0% reduction. Physical transport tiles drop from 660 to 212. The selected layout has
six splitters; the compacted leaf alternative has only five but occupies 1,247 cells. Component
count is not an objective. The score is used bounding-box area, unique physical transport tiles,
then route turns, and only independently validated complete candidates enter selection.

This remains recipe-derived, two-bank, belt-only research. It does not complete the 59-facility
heavy-xiranite graph, handle its cycles or integrate its side pipes. No global minimum-area
claim is made. Rust production construction, validation rules and dependencies are unchanged.

| Case | Previous used bounds | Selected used bounds | Previous / selected physical tiles |
| --- | --- | --- | ---: |
| Merge | 31 x 15 | 17 x 11 | 75 / 35 |
| Four consumers | 72 x 32 | 22 x 31 | 561 / 149 |
| Combined | 73 x 32 | 24 x 37 | 660 / 212 |
| Eight consumers | 212 x 64 | 30 x 57 | 3,553 / 457 |
| Four replicated groups | 77 x 64 | 51 x 55 | 1,216 / 793 |
| Twenty-four consumers | No candidate: leaf cell budget | 62 x 161 | unavailable / 3,489 |

Previous measurements are preserved in `tree-matrix.json` from `cee25bf`. New measurements,
candidate scores, catalog/executable/source hashes and witness hashes are in `compact-matrix.json`.
The twenty-four-consumer case still records the leaf family's materialization failure; shared
trunks generate a separate valid candidate without relaxing capacity, recipe or geometry rules.

## What changed

- Tree primitives use all three available catalog arms. A 1-to-3 or 3-to-1 module occupies
  3 x 3 including its connection cells at pitch one. Recursive composition supports larger
  arity. The junction lower bound `ceil((L-1)/2)` follows from a tree's maximum three children;
  attaining it does not prove minimum area and is never a candidate-selection constraint.
- The second embedding family keeps a commodity root shared through crossings, tapping near
  consumers rather than spreading every leaf across every serial permutation stage. Taps can
  remain two-way when that geometry wins on area. Different commodities cross only in preplanned
  catalog bridges. Source inlet ordering prevents crossing already descending source trunks.
- Event-free whole-grid strip contraction removes empty spacing and straight run interiors.
  It never contracts a facility body, component, terminal, bend, branch, merge, transverse run
  or rate transition. It reconnects straight directed chains with their unchanged exact rate.
  Facility dimensions, component types/rotations, terminal identity and utilization are unchanged.
- The CLI compares raw and compacted leaf/trunk candidates. Independent release-validator
  processes run concurrently, with separate JSON and streamed per-candidate log files. A failed
  candidate cannot replace or discard a valid one. Stdout remains one short completion summary.

The old binary comb is removed, not retained behind a compatibility adapter. A single ternary
tree builder handles split/merge orientations and both catalog transport layers. The existing
leaf and new trunk geometries are explicit experimental alternatives with the same allocation
and lowering contract, not production solver fallbacks.

## Verification and review

- 64 audit expectations: 13 complete recipe-interface cases; three missing-component negative
  controls; 48 primitive probes covering belt/pipe, splitter/converger and every arity 1..12.
  Primitive probes intentionally retain a wholly pending anchor recipe. They prove only their
  covered component geometry, not complete pipe recipe integration.
- 28 Node regression tests cover exact margins, catalog arms/capacity, input-order determinism,
  3,136 ordered approach pairs, non-mutating/idempotent strip contraction, terminal preservation,
  large score inputs and CLI validator/generation failure distinctions.
- `cargo check --workspace` and `cargo test --workspace`: 524 passed, two existing ignores.
  Validation uses the existing optimized executable. No Rust edits or rebuild are required.
- Combined, merge and eight-consumer actual witness screenshots were personally inspected.
  Each page passed six theme/viewport checks, including the clearly marked overloaded view.

An independent read-only review found three actionable issues, each reproduced locally:

1. `Math.max(...points)` fails with 150,000 cells. Replace argument spreading with a bounds
   reduction and isolate per-candidate processing failures. The synthetic helper regression is
   not evidence that the current bounded fixtures reach this size.
2. Bypassed tree reservation coordinates in `five.json` can exceed actual bounds. Keep all old
   reservation coordinates under `pre_compaction_plan`; only transformed actual port coordinates
   remain in compacted port groups. No witness geometry was damaged by the original metadata bug.
3. Empty component catalogs cause both families to fail before validation. Report generation
   failure distinctly, rather than saying the independent validator rejected nonexistent candidates.

No flow/topology corruption or acceptance bypass was confirmed. Remaining concerns are limited
search quality and the explicit local-interface scope, not an infeasibility result.
A second read-only review confirmed those fixes and reran all 13 automatic model/CLI tests.
It noted a remaining test coverage gap: simultaneous candidate-processing exception plus another
candidate's successful validation is handled in code but not directly fault-injected by the suite.

## Measurement and next visible bottleneck

Construction and compaction are measured separately from independent validation and output.
The retained matrix contains fresh-process observations and 20 warm samples after one discarded
run. These development-machine timings are not CPU-isolated and include constructing candidates
that lose on area. Candidate reports separate JSON input I/O, validator process wall time and
render/HTML I/O; validator internal time is recorded separately. No compilation time is included.

| Case | Fresh construction + compaction ms | Warm median ms | Selected validator internal ms |
| --- | ---: | ---: | ---: |
| Combined | 52.81 | 17.33 | 1.35 |
| Eight consumers | 137.43 | 51.70 | 1.88 |
| Twenty-four consumers | 312.80 | 186.80 | 13.45 |

Visual inspection shows a substantial empty upper-right rectangle in the combined trunk result:
all source roots are placed above consumer leaf rows. The eight-consumer result also reserves
distinct bend columns in the exit adapter. Global strip contraction cannot remove these spaces
because other rows/columns contain facilities or bends. The next placement-level experiment
should allow banks to overlap vertically and compact ordered approach geometry locally, retaining
the complete validated candidate until an area-improving replacement validates. Upstream ordering
still uses port pressure/inversions/fragment count as a heuristic; it does not optimize measured
footprint. Capacity partition and placement alternatives should ultimately use actual area too.

Policy check: consistent with `docs/designs/2026-09-02.00-constructive-layout-planner.md` as isolated
research with runtime data, explicit local failures and independent validation. No production
placement-before-routing cutover, global exact fallback, canvas limit or full-factory claim.
