# Two-sided consumer placement

Date: 2026-09-06. Status: verified complete-factory improvement; full goal remains active.

## Outcome

The complete original heavy-xiranite layout shrinks from 117x88 to 112x88: area 10,296 -> 9,856
(4.3% lower). All 59 original facilities, 131 recipe terminal margins and seven original external
obligations pass independent validation. Physical transport tiles increase 2,273 -> 2,299 and turns
245 -> 256. This is still better under the required area-first ranking; component count stays at
fifteen and is not an objective. The old complete incumbent remains available throughout the run.

The improvement does not come from solving the narrower F14 -> F09 obstruction. Personally inspected
the full render: F02 and F03 now use empty space below their suppliers instead of always extending
the rightmost edge. Facility rotations are unchanged; the additional bends are real and visible.

## Candidate contract

The forward family anchors a consumer one cell beyond the rightmost supplier connection and samples
nonnegative horizontal offsets. The explicit two-sided family adds the mirrored negative offsets.
These are offsets relative to the anchor, not negative world coordinates. Both families use the same
vertical offsets, runtime catalog orientation, occupancy checks and exact score order. Each private
candidate must connect all inputs before independent validation and acceptance.

The existing 64-trial and eight-success bounds remain. Increasing the candidate family does not
guarantee monotonic complete-layout improvement: ordering and early greedy choices can change.
The research closure and packing CLIs expose `--placement-family forward|two-sided`, retaining
forward as default. Production behavior and Rust APIs are unchanged. This is not a solver cutover.

Repeated the old successful forward closure: all 37 candidate witness hashes and the final
10,296-area score are identical. The two-sided unit test verifies the expanded geometric slot set
contains every old slot, keeps rotations, respects both transport layers and does not mutate the
retained source witness. Candidate-count bounds still mean not every expanded slot is routed.

## Narrower neighbor diagnosis

At relative cluster offset (23,-45), both two-sided route families stop at 55 facilities. F09 gets
nineteen nonoverlapping placement trials, including the formerly untested (39,66) 5x5 slot. All fail
the enriched-carbon F14 -> F09 corridor. Therefore negative offsets alone do not resolve this case.
The old ghost slot (40,66) overlaps F39 by one column; shifting left removes that body overlap but
does not establish an input approach.

Read-only four-neighbor belt-cell connectivity was checked on the retained 53-facility geometry with
the hypothetical F09 at (39,66). Bodies and same-layer geometry/components are obstacles. In the
nonnegative canvas, the source and target components contain 2,468 and 3,991 reachable cells and do
not connect. This check used a free outer ring beyond the measured positive extent, not just the
existing path-template coordinates. Allowing x/y=-1 produces a 154-cell undirected path, including
x=-1 on the west. It is not a validated factory route: it ignores the current external-boundary
contract and is only a connectivity diagnostic. No global impossibility claim follows. The retained
seed-return belts partition the current canvas; either a planned crossing/escape interface or a
different layout is needed to exploit this narrower neighbor efficiently.

## Whole-factory comparison

The known successful partial packing at offset (24,-45) closes under both two-sided corridor families
at area 9,856. A radius-two comparison around it enumerates 25 translations: seventeen are collision-
free, sixteen retain sufficient immediate ports, and all sixteen pass the necessary free-space
screen. Twelve of 32 full closure attempts complete. The best remains offset (24,-45), now with
two-sided consumer placement. Each purported complete child is independently checked again by the
parent before comparison to the incumbent. Smaller partial scores never enter final selection.

The full comparison takes 87.37 s, including reconstruction, 32 closure attempts, validation and IO,
with two closure workers. Packing itself takes 115.03 ms. Separate known-packing runs take 5.32 s
(simple; 3.73 s generation) and 6.80 s (perimeter; 5.26 s generation). These are closure-only timings
after the retained 53-facility construction, not full fresh-factory latency. Concurrent verification
and different workloads prevent treating these observations as a controlled speed comparison.

## Measured next performance target

Captured a V8 CPU profile for the complete simple two-sided closure with `--cpu-prof`. It reproduces
the same final witness hash. The profile contains 171 samples with 5,147 ms of accumulated sample
deltas. This is a coarse single diagnostic, not a statistically stable attribution benchmark.

| Sample attribution | Approximate time | Share |
| --- | ---: | ---: |
| structuredClone, self | 1,881 ms | 36.5% |
| growCandidates, inclusive | 3,239 ms | 62.9% |
| connect, inclusive | 996 ms | 19.3% |
| routeTemplates, inclusive | 830 ms | 16.1% |
| garbage collection, self | 242 ms | 4.7% |

Inclusive rows overlap and must not be added together. The dominant observed self cost is cloning,
not routing. Each attempted placement currently deep-clones the entire retained witness before
trying any input, including candidates that immediately fail. Next measure those clone calls directly
and test a detached delta/append transaction that materializes a fully independent witness only on
success. Preserve every existing witness hash, candidate ordering and immutability guarantee. Do not
relax geometry checks or introduce shared mutable DTOs merely to reduce copying. Connectivity
screening could also save futile route attempts, but the profile puts full-witness cloning first.

For area, return to the large cluster's wide structured interfaces rather than only making smaller
offset adjustments. The current layout still has substantial visible unused space. Final compactness,
broader recipe coverage, cleanup and the real recipe/rate-to-layout web workflow remain unfinished.

## Verification and reproduction

- 59 related Node tests pass; `cargo check --workspace` passes; 524 Rust tests pass with two existing
  ignores. No Rust behavior changed.
- Eight independent final-factory audit expectations pass, including invalid geometry, missing
  exports, undeclared externals and mismatched utilization/terminal reports.
- Browser checks pass for full/failed views, zoom/pan/Fit and six theme/viewport combinations.
  Full successful and failed states were inspected personally.
- Final source hashes match the implementation; profiled/unprofiled final witness hashes match.
- Reviewed against the constructive design and AGENTS: connected private growth, fixed runtime
  semantics, independent complete acceptance and area-first incumbent retention remain intact.

Evidence: `two-sided-matrix.json`, `two-sided-controls.json`; closure directory artifacts
`two-sided-neighbor.json`, `two-sided-forward-control.json`, `two-sided.cpuprofile`.
Final: `http://100.114.24.86:8765/bound-interconnect-closure-two-sided.html`.
Failed neighbor: `http://100.114.24.86:8765/bound-interconnect-closure-two-sided-blocked.html`.

Use the nesting reproduction command with `--placement-family two-sided --nesting-screen space`,
`--incumbent docs/benchmarks/constructive-cluster-packing/refine-second-matrix.json` and
`--refine-from docs/benchmarks/constructive-cluster-packing/refine-second-matrix.json --refine-radius 2`.
Use a fresh output directory and retain the source details referenced by the prior reports. For a
single closure use its packed `baseline.json` and the same `--placement-family` flag. Profiling uses
Node's `--cpu-prof --cpu-prof-dir=<existing directory> --cpu-prof-name=closure.cpuprofile` before the
closure script path; detailed output stays in JSON/files and stdout remains a short summary.
