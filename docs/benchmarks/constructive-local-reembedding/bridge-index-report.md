# Removing the accidental whole-factory bridge ceiling

## Outcome

The complete Heavy Xiranite incumbent improves from **45x54 / 2,430 area / 411
transport cells / 37 turns** to **41x53 / 2,173 area / 425 cells / 40 turns**.
Area falls 10.58%; the increased wiring is accepted under the unchanged area-first
objective. All 59 facilities, seven components, recipe requirements and original
external margins independently validate. Significant central/right voids and the
bottom seed return remain; this is not a final compactness or global-optimum claim.

Current selection: `bridge-index-results.json`, backed by
`target/bridge-index-final-coalescence/candidate-8291`.

## Current-state controls and exposed failure

The historical upper-bank cooperative experiment is not the current incumbent:
F42 has already moved from (17,7) to (16,3). Replaying that old bank translation
now collides with F42. Carrying F42 too exposes F52's terminal at (6,6); adding the
next facilities collides with other bodies. These failures do not justify bypassing
movement or final-port collision checks. No change to that staging was made.

Instead, independent cone, compound and component-pose portfolios were run from
the current 2,430-area factory. Cone improves wiring to 409 cells / 34 turns;
component poses reach 408 / 36, without reducing area. Compound finds a validated
44x54 candidate at stage 3460, with twelve bridges, then aborts at next-round indexing.
The saved complete candidate survives, but the old runner cannot finish its report.

Cause: an all-bridge connectivity index called the **eight-release transaction API**
with every bridge in the factory. The operational local mutation limit was therefore
accidentally treated as a whole-factory limit. This also affected coalescence's own
topology preparation. Twelve and thirteen bridges occur in valid intermediate layouts;
their counts are not game limits or failure conditions.

## Contract correction

`bridgeTopologyView` projects all structurally supported bridges for read-only
connectivity discovery. `releaseBridgeDraft` still permits at most eight specified
bridges per actual transaction. Both share one private structural projection
implementation; the purpose and budget are no longer conflated. No duplicate game
rule implementation, bridge bypass in a factory witness, or unbounded release action
was added. The index does not represent physical candidate geometry.

Coalescence pairs requiring more than eight incident releases are explicitly skipped
with `local-bridge-release-budget` and entity references. Other pairs continue; no
release set is silently truncated. Remaining routing, port, capacity and full-factory
validation gates are unchanged. See `bridge-index-contract.md`.

The nine-bridge regression fails before the fix with the measured eight-ID error and
passes afterward. Explicit nine-bridge release still rejects. Additional tests verify
that malformed ownership, terminal contact and unequal rates still reject, and an
over-budget pair does not prevent an unrelated pair from producing proposals.

## Verified path to smaller area

The corrected compound run reproduces the old stage-3460 witness exactly, then
continues through twelve- and thirteen-bridge parent states. Its major transitions:

- Stage 3460 exchanges F04 with F21/F15; area becomes 2,376.
- Stage 3946 reorders F15 relative to F12/F07 with a bounded vacant-port repair;
  area becomes 2,322.
- Stage 7777 reverses the horizontal ordering of F01 and F04; reembedding and axis
  compaction produce 42x53 / 2,226 / 466 / 50.

These are actual parent/child transitions. Other selected records within a round can
be siblings and must not be added into a fictional cumulative movement sequence.
The fix enables continuation through valid intermediate states; it does not change
the first area-improving proposal or claim that every later gain has one sole cause.

| Experiment | Starting area | Trials | Best area / cells / turns | Seconds |
| --- | ---: | ---: | --- | ---: |
| Corrected compound, three rounds | 2,430 | 11,190 | 2,226 / 466 / 50 | 40.26 |
| Coalescence from saved twelve-bridge candidate | 2,376 | 5,334 | 2,322 / 441 / 46 | 36.27 |
| Further compound | 2,226 | 11,218 | 2,173 / 446 / 50 | 36.41 |
| Further component poses | 2,226 | 1,536 | 2,226 / 459 / 48 | 11.11 |
| Further coalescence | 2,226 | 10,345 | **2,173 / 425 / 40** | 64.65 |

Independent branches were evaluated concurrently. Times are observed optimization
batch costs, not initial factory construction latency, stable speedups or summed
end-to-end wall time. Final compound and coalescence both terminate on an unimproved
bounded neighborhood; this does not prove local or global optimality. The final
coalescence is selected because it ties the best area and uses fewer transport cells.

Reports are respectively `target/current-pose-compound-fixed/report.json`,
`target/bridge-index-coalescence/report.json`, and the three
`target/bridge-index-final-{compound,components,coalescence}/report.json` files.
The initial `target/current-pose-compound` attempt supplied the wrong component data
path and failed before useful execution; it is excluded. The correctly configured
`target/current-pose-compound-final` is the distinct, genuine bridge-index failure.

## Verification and visual review

230 Node tests pass, including the red/green regression. Cargo check and 524 Rust
tests pass, with two existing ignores. Eight original-graph controls and sixteen
crossing controls pass on the final selected factory. No accepted candidate in the
five post-fix runs is rejected by the independent validation gate. Engine source
hashes in all five reports match the final implementation. No Rust edits/dependencies.

Native full renders were personally inspected for initial cone/component controls,
the saved twelve-bridge candidate, both first continuations, and all three final
branches. Focused inspection covered the saved candidate's tangled upper mixed-transport
head, the corrected 42x53 head, the final selected head, and the lower splitter region
in the initial component control. All eight pages pass six viewport/theme checks.
The selected head removes the interim bridge cluster, while the central lower void
and bottom return loop remain visibly costly.

Selected viewer:
`http://100.114.24.86:8765/bound-interconnect-closure-bridge-index-final-coalescence.html`.
The viewer uses the actual validated witness, not a hand-edited drawing.

Compared with AGENTS.md and the constructive-planner design, the simplification is
the separation of read-only topology inspection from bounded physical mutation.
Production growth, external obligations, transaction validation, score order and
the research/production boundary are unchanged. This remains an isolated improvement
experiment; no premature production or Rust cutover is claimed.

## Next work

Keep the new complete incumbent and target the central lower vacancy and long return
connections. Test geometry-driven bounded group closure/packing alongside the existing
logical groups, and evaluate final-state placement/port staging only with a concrete
valid regression; do not weaken collision checks based on the obsolete upper-bank
case. An eight-bridge *release* budget is still intentional and reported. Broader
recipe coverage, cleanup and the web product remain open.
